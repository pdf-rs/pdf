use super::prelude::*;

#[derive(DataSize, Debug)]
pub struct NumberTree<T> {
    pub limits: Option<(i32, i32)>,
    pub node: NumberTreeNode<T>,
}

#[derive(DataSize, Debug)]
pub enum NumberTreeNode<T> {
    Leaf(Vec<(i32, T)>),
    Intermediate(Vec<Ref<NumberTree<T>>>),
}
impl<T: Object> Object for NumberTree<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.resolve(resolve)?.into_dictionary()?;

        let limits = match dict.remove("Limits") {
            Some(limits) => {
                let limits = t!(limits.resolve(resolve)?.into_array());
                if limits.len() != 2 {
                    bail!("Error reading NameTree: 'Limits' is not of length 2");
                }
                let min = t!(limits[0].as_integer());
                let max = t!(limits[1].as_integer());

                Some((min, max))
            }
            None => None,
        };

        let kids = dict.remove("Kids");
        let nums = dict.remove("Nums");
        match (kids, nums) {
            (Some(kids), _) => {
                let kids = t!(kids
                    .resolve(resolve)?
                    .into_array()?
                    .iter()
                    .map(|kid| Ref::<NumberTree<T>>::from_primitive(kid.clone(), resolve))
                    .collect::<Result<Vec<_>>>());
                Ok(NumberTree {
                    limits,
                    node: NumberTreeNode::Intermediate(kids),
                })
            }
            (None, Some(nums)) => {
                let list = nums.into_array()?;
                let mut items = Vec::with_capacity(list.len() / 2);
                for (key, item) in list.into_iter().tuples() {
                    let idx = t!(key.as_integer());
                    let val = t!(T::from_primitive(item, resolve));
                    items.push((idx, val));
                }
                Ok(NumberTree {
                    limits,
                    node: NumberTreeNode::Leaf(items),
                })
            }
            (None, None) => {
                warn!("Neither Kids nor Names present in NumberTree node.");
                Ok(NumberTree {
                    limits,
                    node: NumberTreeNode::Intermediate(vec![]),
                })
            }
        }
    }
}
impl<T: ObjectWrite> ObjectWrite for NumberTree<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut dict = Dictionary::new();
        if let Some(limits) = self.limits {
            dict.insert("Limits", vec![limits.0.into(), limits.1.into()]);
        }
        match self.node {
            NumberTreeNode::Leaf(ref items) => {
                let mut nums = Vec::with_capacity(items.len() * 2);
                for &(idx, ref label) in items {
                    nums.push(idx.into());
                    nums.push(label.to_primitive(update)?);
                }
                dict.insert("Nums", nums);
            }
            NumberTreeNode::Intermediate(ref kids) => {
                dict.insert(
                    "Kids",
                    kids.iter().map(|r| r.get_inner().into()).collect_vec(),
                );
            }
        }
        Ok(dict.into())
    }
}
impl<T: Object + DataSize> NumberTree<T> {
    pub fn walk(&self, r: &impl Resolve, callback: &mut dyn FnMut(i32, &T)) -> Result<()> {
        match self.node {
            NumberTreeNode::Leaf(ref items) => {
                for &(idx, ref val) in items {
                    callback(idx, val);
                }
            }
            NumberTreeNode::Intermediate(ref items) => {
                for &tree_ref in items {
                    let tree = r.get(tree_ref)?;
                    tree.walk(r, callback)?;
                }
            }
        }
        Ok(())
    }
}
