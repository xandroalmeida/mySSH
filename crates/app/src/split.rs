//! Árvore binária de divisões (painéis) de uma aba. Lógica pura: sem sessões nem UI.

use egui::{Pos2, Rect, vec2};

pub type PaneId = u64;

/// Direção da divisão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Divisor vertical: painéis lado a lado.
    Vertical,
    /// Divisor horizontal: painéis empilhados.
    Horizontal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Leaf(PaneId),
    Split {
        dir: Dir,
        /// Fração do espaço (sem o divisor) dada ao primeiro filho.
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

/// Caminho até um nó: `false` = primeiro filho, `true` = segundo.
pub type Path = Vec<bool>;

#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    pub path: Path,
    pub dir: Dir,
    pub rect: Rect,
    /// Retângulo do nó pai (para converter a posição do ponteiro em fração).
    pub parent: Rect,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub panes: Vec<(PaneId, Rect)>,
    pub dividers: Vec<Divider>,
}

/// Menor tamanho de um painel (em pontos) ao arrastar o divisor.
pub const MIN_PANE: f32 = 120.0;
/// Espessura do divisor.
pub const GAP: f32 = 6.0;

/// Limita a fração para que nenhum lado fique menor que `min` (se couber, senão centraliza).
pub fn clamp_ratio(ratio: f32, total: f32, min: f32) -> f32 {
    if total <= 0.0 {
        return 0.5;
    }
    let lo = (min / total).min(0.5);
    ratio.clamp(lo, 1.0 - lo)
}

impl Node {
    pub fn leaves(&self) -> Vec<PaneId> {
        let mut v = Vec::new();
        self.collect(&mut v);
        v
    }

    fn collect(&self, out: &mut Vec<PaneId>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { a, b, .. } => {
                a.collect(out);
                b.collect(out);
            }
        }
    }

    #[cfg(test)]
    pub fn contains(&self, id: PaneId) -> bool {
        match self {
            Node::Leaf(x) => *x == id,
            Node::Split { a, b, .. } => a.contains(id) || b.contains(id),
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        match self {
            Node::Leaf(_) => 1,
            Node::Split { a, b, .. } => a.count() + b.count(),
        }
    }

    /// Divide a folha `target`: ela fica no primeiro lado e `new` no segundo.
    pub fn split_leaf(&mut self, target: PaneId, dir: Dir, new: PaneId) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split {
                    dir,
                    ratio: 0.5,
                    a: Box::new(Node::Leaf(target)),
                    b: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => {
                a.split_leaf(target, dir, new) || b.split_leaf(target, dir, new)
            }
        }
    }

    /// Remove a folha; o irmão ocupa o lugar do pai. `None` se não restou nada.
    pub fn remove(self, id: PaneId) -> Option<Node> {
        match self {
            Node::Leaf(x) if x == id => None,
            leaf @ Node::Leaf(_) => Some(leaf),
            Node::Split { dir, ratio, a, b } => match (a.remove(id), b.remove(id)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    dir,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }

    pub fn ratio_mut(&mut self, path: &[bool]) -> Option<&mut f32> {
        match (self, path) {
            (Node::Split { ratio, .. }, []) => Some(ratio),
            (Node::Split { a, b, .. }, [first, rest @ ..]) => {
                if *first {
                    b.ratio_mut(rest)
                } else {
                    a.ratio_mut(rest)
                }
            }
            _ => None,
        }
    }

    /// Calcula os retângulos dos painéis e dos divisores dentro de `rect`.
    pub fn layout(&self, rect: Rect) -> Layout {
        let mut out = Layout::default();
        self.layout_into(rect, &mut Vec::new(), &mut out);
        out
    }

    fn layout_into(&self, rect: Rect, path: &mut Path, out: &mut Layout) {
        match self {
            Node::Leaf(id) => out.panes.push((*id, rect)),
            Node::Split { dir, ratio, a, b } => {
                let (ra, rd, rb) = match dir {
                    Dir::Vertical => {
                        let avail = (rect.width() - GAP).max(0.0);
                        let r = clamp_ratio(*ratio, avail, MIN_PANE);
                        let wa = avail * r;
                        (
                            Rect::from_min_size(rect.min, vec2(wa, rect.height())),
                            Rect::from_min_size(
                                Pos2::new(rect.min.x + wa, rect.min.y),
                                vec2(GAP, rect.height()),
                            ),
                            Rect::from_min_size(
                                Pos2::new(rect.min.x + wa + GAP, rect.min.y),
                                vec2(avail - wa, rect.height()),
                            ),
                        )
                    }
                    Dir::Horizontal => {
                        let avail = (rect.height() - GAP).max(0.0);
                        let r = clamp_ratio(*ratio, avail, MIN_PANE / 2.0);
                        let ha = avail * r;
                        (
                            Rect::from_min_size(rect.min, vec2(rect.width(), ha)),
                            Rect::from_min_size(
                                Pos2::new(rect.min.x, rect.min.y + ha),
                                vec2(rect.width(), GAP),
                            ),
                            Rect::from_min_size(
                                Pos2::new(rect.min.x, rect.min.y + ha + GAP),
                                vec2(rect.width(), avail - ha),
                            ),
                        )
                    }
                };
                out.dividers.push(Divider {
                    path: path.clone(),
                    dir: *dir,
                    rect: rd,
                    parent: rect,
                });
                path.push(false);
                a.layout_into(ra, path, out);
                path.pop();
                path.push(true);
                b.layout_into(rb, path, out);
                path.pop();
            }
        }
    }
}

impl Divider {
    /// Fração correspondente à posição do ponteiro (centro do divisor sobre `pos`).
    pub fn ratio_at(&self, pos: Pos2) -> f32 {
        match self.dir {
            Dir::Vertical => {
                let avail = (self.parent.width() - GAP).max(1.0);
                clamp_ratio(
                    (pos.x - self.parent.min.x - GAP / 2.0) / avail,
                    avail,
                    MIN_PANE,
                )
            }
            Dir::Horizontal => {
                let avail = (self.parent.height() - GAP).max(1.0);
                clamp_ratio(
                    (pos.y - self.parent.min.y - GAP / 2.0) / avail,
                    avail,
                    MIN_PANE / 2.0,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(806.0, 406.0))
    }

    fn tree() -> Node {
        // 1 | (2 / 3)
        let mut t = Node::Leaf(1);
        assert!(t.split_leaf(1, Dir::Vertical, 2));
        assert!(t.split_leaf(2, Dir::Horizontal, 3));
        t
    }

    #[test]
    fn dividir_cria_irmaos_na_ordem() {
        let t = tree();
        assert_eq!(t.leaves(), vec![1, 2, 3]);
        assert_eq!(t.count(), 3);
        assert!(t.contains(3));
        assert!(!t.contains(9));
        let mut t2 = t.clone();
        assert!(!t2.split_leaf(9, Dir::Vertical, 10));
        assert_eq!(t2, t);
    }

    #[test]
    fn remover_promove_o_irmao() {
        let t = tree().remove(2).unwrap();
        assert_eq!(t.leaves(), vec![1, 3]);
        let t = t.remove(1).unwrap();
        assert_eq!(t, Node::Leaf(3));
        assert_eq!(t.remove(3), None);
        // id inexistente não muda nada
        assert_eq!(tree().remove(42), Some(tree()));
    }

    #[test]
    fn layout_cobre_o_espaco_sem_sobrepor() {
        let l = tree().layout(rect());
        assert_eq!(l.panes.len(), 3);
        assert_eq!(l.dividers.len(), 2);
        let (_, r1) = l.panes[0];
        let (_, r2) = l.panes[1];
        let (_, r3) = l.panes[2];
        assert_eq!(r1.min, Pos2::ZERO);
        assert!((r1.width() - 400.0).abs() < 0.01);
        assert!((r2.min.x - 406.0).abs() < 0.01);
        assert!((r2.width() - 400.0).abs() < 0.01);
        assert!(r2.max.y <= r3.min.y);
        assert!((r3.max.y - 406.0).abs() < 0.01);
        assert!(!r1.intersects(r2) && !r2.intersects(r3));
    }

    #[test]
    fn razao_respeita_minimo() {
        assert_eq!(clamp_ratio(0.01, 800.0, MIN_PANE), MIN_PANE / 800.0);
        assert_eq!(clamp_ratio(0.99, 800.0, MIN_PANE), 1.0 - MIN_PANE / 800.0);
        // espaço pequeno demais: centraliza
        assert_eq!(clamp_ratio(0.9, 100.0, MIN_PANE), 0.5);
    }

    #[test]
    fn arrastar_divisor_ajusta_a_razao() {
        let mut t = tree();
        let l = t.layout(rect());
        let d = l.dividers[0].clone();
        assert_eq!(d.path, Vec::<bool>::new());
        let r = d.ratio_at(Pos2::new(203.0, 10.0));
        assert!((r - 0.25).abs() < 0.01);
        *t.ratio_mut(&d.path).unwrap() = r;
        let l = t.layout(rect());
        assert!((l.panes[0].1.width() - 200.0).abs() < 0.5);
        // divisor interno (caminho [true])
        assert_eq!(l.dividers[1].path, vec![true]);
        assert!(t.ratio_mut(&[true]).is_some());
        assert!(t.ratio_mut(&[false]).is_none());
    }
}
