//! Saída remota hostil não pode inflar a memória do terminal (DoS):
//! - OSC sem terminador (o `vte` acumula o payload sem limite);
//! - `CSI 22 t` repetido (push de título clona o título até 4096 vezes).
//!
//! Mede os bytes vivos no heap com um alocador contador. Este arquivo tem um único teste (o
//! contador é global ao processo).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use myssh_ssh::{SessionCommand, SessionEvent, SessionHandle};
use myssh_term::TermSession;
use tokio::sync::mpsc;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn bump(n: usize) {
    let now = LIVE.fetch_add(n, Ordering::Relaxed) + n;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        if new > l.size() {
            bump(new - l.size());
        } else {
            LIVE.fetch_sub(l.size() - new, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn session() -> (TermSession, mpsc::Sender<SessionEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<SessionCommand>(64);
    std::mem::forget(cmd_rx);
    let (evt_tx, evt_rx) = mpsc::channel(4);
    (
        TermSession::new(SessionHandle::new(cmd_tx, evt_rx), 80, 24),
        evt_tx,
    )
}

fn feed(s: &mut TermSession, tx: &mpsc::Sender<SessionEvent>, data: Vec<u8>) {
    tx.try_send(SessionEvent::Data(data)).unwrap();
    while s.pump(1 << 20) {}
}

#[test]
fn memoria_limitada_com_saida_hostil() {
    const MIB: usize = 1024 * 1024;

    // 1) 50 MB de OSC sem terminador, em lotes de 64 KiB.
    let (mut s, tx) = session();
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    feed(&mut s, &tx, b"\x1b]0;".to_vec());
    for _ in 0..(50 * MIB / 65536) {
        feed(&mut s, &tx, vec![b'A'; 65536]);
    }
    let peak_osc = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    println!(
        "OSC 50 MiB sem terminador: pico de heap = {} KiB",
        peak_osc / 1024
    );
    assert!(peak_osc < 4 * MIB, "pico {} KiB", peak_osc / 1024);
    // Depois do terminador o terminal continua funcional.
    feed(&mut s, &tx, b"\x07ok".to_vec());
    assert!(s.screen_text().contains("ok"));

    // 2) Título grande + 10k pushes de título.
    let (mut s, tx) = session();
    let mut t = b"\x1b]2;".to_vec();
    t.extend(std::iter::repeat_n(b'T', 4000));
    t.push(7);
    feed(&mut s, &tx, t);
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    feed(&mut s, &tx, b"\x1b[22;0t".repeat(10_000));
    let peak_push = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    println!("10k CSI 22 t: pico de heap = {} KiB", peak_push / 1024);
    assert!(peak_push < MIB, "pico {} KiB", peak_push / 1024);

    // Título normal continua funcionando.
    feed(&mut s, &tx, b"\x1b]2;meu t\xc3\xadtulo\x07".to_vec());
    assert_eq!(s.title(), Some("meu título"));
}
