use core::mem;

use alloc::boxed::Box;

/// This is a pairing max heap.
///
/// Unlike BinaryHeap, this doesn't use a large continuous allocation. Instead this will do lots of
/// small per element allocations. Therefore this'll perform worse, but there won't be reallocation
/// stutters.
pub struct PairingHeap<T: Ord> {
    root: Option<Box<Node<T>>>,
    num_merges: usize,
    len: usize,
}

struct Node<T: Ord> {
    left_child: Option<Box<Node<T>>>,
    right_sibiling: Option<Box<Node<T>>>,
    value: T,
}

impl<T: Ord> Node<T> {
    const fn new(value: T) -> Self {
        Self {
            left_child: None,
            right_sibiling: None,
            value,
        }
    }
}

fn meld<T: Ord>(root: &mut Box<Node<T>>, mut child: Box<Node<T>>) {
    debug_assert!(root.right_sibiling.is_none());
    debug_assert!(child.right_sibiling.is_none());

    if root.value < child.value {
        mem::swap(&mut child, root);
    }
    let sibiling = root.left_child.take();
    root.left_child.insert(child).right_sibiling = sibiling;
}

fn merge_pairs<T: Ord>(root: &mut Box<Node<T>>) {
    let Some(mut node1) = root.right_sibiling.take() else {
        return;
    };
    let node2 = node1.right_sibiling.take();
    meld(root, node1);
    if let Some(mut node2) = node2 {
        merge_pairs(&mut node2);
        meld(root, node2);
    }
}

impl<T: Ord> PairingHeap<T> {
    pub const fn new() -> Self {
        Self {
            root: None,
            len: 0,
            num_merges: 0,
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn peek(&mut self) -> Option<&T> {
        let root = self.root.as_mut()?;
        self.num_merges = 0;
        merge_pairs(root);
        Some(&root.value)
    }

    pub fn push(&mut self, value: T) {
        self.len += 1;
        self.num_merges += 1;

        let mut node = Box::new(Node::new(value));

        let Some(root) = &mut self.root else {
            self.root = Some(node);
            self.num_merges = 0;
            return;
        };

        if root.value <= node.value {
            mem::swap(root, &mut node);
            root.left_child = Some(node);
            self.num_merges = 0;
            return;
        }
        node.right_sibiling = root.right_sibiling.take();
        root.right_sibiling = Some(node);

        for _ in 0..self.num_merges.trailing_zeros() {
            let Some(mut node1) = root.right_sibiling.take() else {
                break;
            };
            let Some(mut node2) = node1.right_sibiling.take() else {
                break;
            };
            let node3 = node2.right_sibiling.take();
            meld(&mut node1, node2);
            node1.right_sibiling = node3;
            let node1 = root.right_sibiling.insert(node1);
            if node1.right_sibiling.is_none() {
                break;
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        let mut root = self.root.take()?;
        merge_pairs(&mut root);
        self.num_merges = 0;
        self.len -= 1;
        self.root = root.left_child.take();
        Some(root.value)
    }

    /// Usually faster than `pop()`
    pub fn pop_any(&mut self) -> Option<T> {
        let mut root = self.root.take()?;
        if let Some(mut node) = root.right_sibiling.take() {
            if let Some(mut child) = node.left_child.take() {
                merge_pairs(&mut child);
                root.right_sibiling.insert(child).right_sibiling = node.right_sibiling.take();
            } else {
                root.right_sibiling = node.right_sibiling.take();
            }
            self.root = Some(root);

            Some(node.value)
        } else {
            self.root = root.left_child.take();
            Some(root.value)
        }
    }
}
