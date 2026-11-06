use std::collections::{HashSet, VecDeque};

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
    pub fn build_tree(mut entries: Vec<(PdfString, T)>) -> Result<Self> {
        const IDEAL_LEVEL_SIZE: f32 = 17.;
        let len = entries.len();
        if len < 20 {
            return Ok(Self::build_flat(entries));
        }
        let levels = (len as f32).log2() / IDEAL_LEVEL_SIZE.log2();
        let root_levels_ln2 = levels.fract();
        let root_levels = IDEAL_LEVEL_SIZE.powf(root_levels_ln2).ceil() as usize;
        entries.sort_unstable_by(|a, b| a.0.data.as_slice().cmp(b.0.data.as_slice()));
        let mut entries = VecDeque::from(entries);

        unimplemented!()
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
