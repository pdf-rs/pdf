use std::collections::HashSet;

use super::prelude::*;

/// A `/Kids` chain deeper than this is treated as broken: 64 levels is far
/// beyond what any writer produces, and it bounds the recursion.
const MAX_NAME_TREE_DEPTH: usize = 64;

/// A page-label numbering style (`/S` in a `PageLabel` dictionary).
///
/// The `/S` names are case-sensitive per PDF 32000-1 §12.4.2 Table 159:
/// `R`/`r` are upper/lower-case Roman numerals and `A`/`a` are upper/lower-case
/// letters respectively.
#[derive(Debug, DataSize, Clone, Object, ObjectWrite, DeepClone)]
pub enum Counter {
    #[pdf(name = "D")]
    Arabic,
    #[pdf(name = "R")]
    RomanUpper,
    #[pdf(name = "r")]
    RomanLower,
    #[pdf(name = "A")]
    AlphaUpper,
    #[pdf(name = "a")]
    AlphaLower,
}

#[derive(Debug, DataSize)]
pub enum NameTreeNode<T> {
    ///
    Intermediate(Vec<Ref<NameTree<T>>>),
    ///
    Leaf(Vec<(PdfString, T)>),
}
/// Note: The PDF concept of 'root' node is an intermediate or leaf node which has no 'Limits'
/// entry. Hence, `limits`,
#[derive(Debug, DataSize)]
pub struct NameTree<T> {
    pub limits: Option<(PdfString, PdfString)>,
    pub node: NameTreeNode<T>,
}
impl<T: Object + DataSize> NameTree<T> {
    pub fn walk(&self, r: &impl Resolve, callback: &mut dyn FnMut(&PdfString, &T)) -> Result<()> {
        // A name tree is a tree in a well-formed file, but `/Kids` may point
        // back at a node that is already being visited. Without a guard that is
        // an endless walk (and a stack overflow once the depth is reached), and
        // with a wide node it is exponential, so a depth limit alone is not
        // enough: remember every node that has been entered.
        let mut seen = HashSet::new();
        self.walk_limited(r, callback, &mut seen, MAX_NAME_TREE_DEPTH)
    }
    fn walk_limited(
        &self,
        r: &impl Resolve,
        callback: &mut dyn FnMut(&PdfString, &T),
        seen: &mut HashSet<ObjNr>,
        depth: usize,
    ) -> Result<()> {
        if depth == 0 {
            bail!("name tree: /Kids chain is deeper than {MAX_NAME_TREE_DEPTH}");
        }
        match self.node {
            NameTreeNode::Leaf(ref items) => {
                for (name, val) in items {
                    callback(name, val);
                }
            }
            NameTreeNode::Intermediate(ref items) => {
                for &tree_ref in items {
                    if !seen.insert(tree_ref.get_inner().id) {
                        bail!("name tree: /Kids contains object {} more than once", tree_ref.get_inner().id);
                    }
                    let tree = r.get(tree_ref)?;
                    tree.walk_limited(r, callback, seen, depth - 1)?;
                }
            }
        }
        Ok(())
    }
}

impl<T: Object> Object for NameTree<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = t!(p.resolve(resolve)?.into_dictionary());

        let limits = match dict.remove("Limits") {
            Some(limits) => {
                let limits = limits.resolve(resolve)?.into_array()?;
                if limits.len() != 2 {
                    bail!("Error reading NameTree: 'Limits' is not of length 2");
                }
                let min = limits[0].clone().into_string()?;
                let max = limits[1].clone().into_string()?;

                Some((min, max))
            }
            None => None,
        };

        let kids = dict.remove("Kids");
        let names = dict.remove("Names");
        // If no `kids`, try `names`. Else there is an error.
        Ok(match (kids, names) {
            (Some(kids), _) => {
                let kids = t!(kids
                    .resolve(resolve)?
                    .into_array()?
                    .iter()
                    .map(|kid| Ref::<NameTree<T>>::from_primitive(kid.clone(), resolve))
                    .collect::<Result<Vec<_>>>());
                NameTree {
                    limits,
                    node: NameTreeNode::Intermediate(kids),
                }
            }
            (None, Some(names)) => {
                let names = names.resolve(resolve)?.into_array()?;
                let mut new_names = Vec::new();
                for pair in names.chunks_exact(2) {
                    let name = pair[0].clone().resolve(resolve)?.into_string()?;
                    let value = t!(T::from_primitive(pair[1].clone(), resolve));
                    new_names.push((name, value));
                }
                NameTree {
                    limits,
                    node: NameTreeNode::Leaf(new_names),
                }
            }
            (None, None) => {
                warn!("Neither Kids nor Names present in NameTree node.");
                NameTree {
                    limits,
                    node: NameTreeNode::Intermediate(vec![]),
                }
            }
        })
    }
}

impl<T: ObjectWrite> ObjectWrite for NameTree<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut dict = Dictionary::new();
        if let Some(ref limits) = self.limits {
            dict.insert("Limits", limits.to_primitive(update)?);
        }
        match self.node {
            NameTreeNode::Intermediate(ref kids) => {
                dict.insert("Kids", kids.to_primitive(update)?);
            }
            NameTreeNode::Leaf(ref children) => {
                let mut list = Vec::with_capacity(children.len() * 2);
                for (key, val) in children {
                    list.push(Primitive::String(key.clone()));
                    let val = val.to_primitive(update)?;
                    match val {
                        Primitive::Null | Primitive::Name(_) | Primitive::Number(_) | Primitive::Boolean(_) => {
                            list.push(val);
                        }
                        _ => {
                            list.push(Primitive::Reference(update.create(val)?.inner));
                        }
                    }
                }
                dict.insert("Names", list);
            }
        }
        Ok(dict.into())
    }
}
impl<T> NameTree<T> {
    pub fn build_flat(mut entries: Vec<(PdfString, T)>) -> Self {
        entries.sort_unstable_by(|a, b| a.0.data.as_slice().cmp(b.0.data.as_slice()));
        let node = NameTreeNode::Leaf(entries);
        NameTree { limits: None, node }
    }
}

impl<T: ObjectWrite> NameTree<T> {
    /// A leaf holds at most this many entries. 7.9.6 caps a node at 64; a
    /// smaller one keeps a lookup to a level or two and is what writers emit.
    const LEAF_SIZE: usize = 17;
    /// Intermediate nodes hold at most this many `/Kids`.
    const KIDS_PER_NODE: usize = 17;

    /// Build a balanced name tree over `entries` (7.9.6): sorted leaves with
    /// `/Limits`, grouped under intermediate nodes that are created in
    /// `update`. The returned node is the root, which the caller owns and
    /// creates itself.
    ///
    /// This used to be `unimplemented!()` for twenty entries or more, which is
    /// to say for every input that needs a tree at all.
    pub fn build_tree(entries: Vec<(PdfString, T)>, update: &mut impl Updater) -> Result<Self> {
        let mut entries = entries;
        entries.sort_unstable_by(|a, b| a.0.data.as_slice().cmp(b.0.data.as_slice()));

        // A level is (node, smallest key below it, largest key below it); the
        // keys are tracked here because `/Kids` holds references, which say
        // nothing about their own `/Limits`.
        let mut level: Vec<(Self, PdfString, PdfString)> = Vec::new();
        let mut rest = entries;
        while !rest.is_empty() {
            let tail = rest.split_off(std::cmp::min(Self::LEAF_SIZE, rest.len()));
            let first = rest.first().map(|(key, _)| key.clone());
            let last = rest.last().map(|(key, _)| key.clone());
            if let (Some(first), Some(last)) = (first, last) {
                let limits = Some((first.clone(), last.clone()));
                level.push((NameTree { limits, node: NameTreeNode::Leaf(rest) }, first, last));
            }
            rest = tail;
        }

        while level.len() > 1 {
            let mut parents = Vec::with_capacity((level.len() + Self::KIDS_PER_NODE - 1) / Self::KIDS_PER_NODE);
            let mut rest = std::mem::take(&mut level);
            while !rest.is_empty() {
                let tail = rest.split_off(std::cmp::min(Self::KIDS_PER_NODE, rest.len()));
                let min = rest[0].1.clone();
                let max = rest[rest.len() - 1].2.clone();
                let kids = rest
                    .into_iter()
                    .map(|(node, _, _)| update.create(node).map(|rc| rc.get_ref()))
                    .collect::<Result<Vec<_>>>()?;
                parents.push((
                    NameTree { limits: Some((min.clone(), max.clone())), node: NameTreeNode::Intermediate(kids) },
                    min,
                    max,
                ));
                rest = tail;
            }
            level = parents;
        }

        match level.pop() {
            // 7.9.6: the root of a name tree has no `/Limits` — it is the one
            // node that describes nothing above itself.
            Some((mut root, _, _)) => {
                root.limits = None;
                Ok(root)
            }
            // No entries at all: an empty leaf, which is a valid name tree.
            None => Ok(NameTree { limits: None, node: NameTreeNode::Leaf(Vec::new()) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Counter;
    use crate::object::{NoResolve, Object};
    use crate::primitive::Primitive;

    fn parse(name: &str) -> Counter {
        Counter::from_primitive(Primitive::Name(name.into()), &NoResolve).unwrap()
    }

    // The `/S` page-label style names are case-sensitive (PDF 32000-1 §12.4.2):
    // `R`/`r` are upper/lower-case Roman, `A`/`a` upper/lower-case letters.
    #[test]
    fn counter_style_names_are_case_sensitive() {
        assert!(matches!(parse("D"), Counter::Arabic));
        assert!(matches!(parse("R"), Counter::RomanUpper));
        assert!(matches!(parse("r"), Counter::RomanLower));
        assert!(matches!(parse("A"), Counter::AlphaUpper));
        assert!(matches!(parse("a"), Counter::AlphaLower));
    }
}
