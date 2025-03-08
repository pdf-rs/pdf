use super::prelude::*;

#[derive(Debug, DataSize, Clone, Object, ObjectWrite, DeepClone)]
pub enum Counter {
    #[pdf(name = "D")]
    Arabic,
    #[pdf(name = "r")]
    RomanUpper,
    #[pdf(name = "R")]
    RomanLower,
    #[pdf(name = "a")]
    AlphaUpper,
    #[pdf(name = "A")]
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
        match self.node {
            NameTreeNode::Leaf(ref items) => {
                for (name, val) in items {
                    callback(name, val);
                }
            }
            NameTreeNode::Intermediate(ref items) => {
                for &tree_ref in items {
                    let tree = r.get(tree_ref)?;
                    tree.walk(r, callback)?;
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
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        todo!("impl ObjectWrite for NameTree")
    }
}
