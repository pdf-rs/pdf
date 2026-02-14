extern crate pdf;

use std::collections::HashMap;
use std::env::args;
use std::marker::PhantomData;
use std::sync::Arc;

use datasize::DataSize;
use pdf::content::{serialize_ops, FormXObject, Op};
use pdf::error::{PdfError, Result};
use pdf::file::FileOptions;
use pdf::font::{Font, FontData, TFont};
use pdf::{object::*, try_opt};
use pdf::primitive::{Name, PdfString, Primitive};

/// Experimental, DO NOT USE
pub trait Access: Sized {
    type Inner;
    type Out;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&Self::Inner) -> Result<Option<Self::Inner>>) -> Result<Option<Self::Out>>;
    fn replace(&self, update: &mut impl Updater, val: Self::Inner) -> Result<Option<Self::Out>>;
}
impl<V: Access<Out=V>> Access for Option<V> {
    type Inner = V::Inner;
    type Out = Self;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&Self::Inner) -> Result<Option<Self::Inner>>) -> Result<Option<Self::Out>> {
        let inner = try_opt!(self);
        inner.maybe_update(resolve, update_inner).map(Some)
    }
    fn replace(&self, update: &mut impl Updater, val: Self::Inner) -> Result<Option<Self::Out>> {
        let inner = try_opt!(self);
        inner.replace(update, val).map(Some)
    }
}
impl<T: ObjectWrite> Access for MaybeRef<T> {
    type Inner = T;
    type Out = Self;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&T) -> Result<Option<T>>) -> Result<Option<Self::Out>> {
        match self {
            MaybeRef::Direct(inner) => {
                match update_inner(inner)? {
                    None => Ok(None),
                    Some(inner) => Ok(Some(MaybeRef::Direct(Arc::new(inner))))
                }
            }
            MaybeRef::Indirect(r) => {
                match update_inner(&*r)? {
                    None => Ok(None),
                    Some(new) => {
                        resolve.update_ref(r, new)?;
                        Ok(None)
                    }
                }
            }
        }
    }
    fn replace(&self, update: &mut impl Updater, val: T) -> Result<Option<Self::Out>> {
        match self {
            MaybeRef::Direct(_) => Ok(Some(MaybeRef::Direct(Arc::new(val)))),
            MaybeRef::Indirect(old) => {
                update.update_ref(old, val)?;
                Ok(None)
            }
        }
    }
}

impl<T: Object + ObjectWrite + DataSize> Access for Lazy<T> {
    type Inner = T;
    type Out = Self;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&T) -> Result<Option<T>>) -> Result<Option<Self::Out>> {
        let inner = self.load(resolve)?;
        match update_inner(&*inner)? {
            None => return Ok(None),
            Some(new) => {
                Ok(Some(Lazy::new(new, resolve)?))
            }
        }
    }
    fn replace(&self, update: &mut impl Updater, val: T) -> Result<Option<Self::Out>> {
        match self.primitive {
            Primitive::Reference(r) => {
                update.update(r, val)?;
                Ok(None)
            }
            _ => Ok(Some(Lazy::new(val, update)?))
        }
    }
}
impl<T: Object + ObjectWrite + DataSize> Access for Ref<T> {
    type Inner = T;
    type Out = Self;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&T) -> Result<Option<T>>) -> Result<Option<Self>> {
        let val = resolve.get(*self)?;
        Ok(val.maybe_update(resolve, update_inner)?.map(|rc| rc.get_ref()))
    }
    fn replace(&self, update: &mut impl Updater, val: T) -> Result<Option<Self>> {
        update.update(self.get_inner(), val)?;
        Ok(None)
    }
}
impl<T: Object + ObjectWrite + DataSize> Access for RcRef<T> {
    type Inner = T;
    type Out = Self;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&T) -> Result<Option<T>>) -> Result<Option<Self>> {
        match update_inner(&*self)? {
            None => Ok(None),
            Some(new) => {
                resolve.update_ref(self, new)?;
                Ok(None)
            }
        }
    }
    fn replace(&self, update: &mut impl Updater, val: T) -> Result<Option<Self>> {
        update.update_ref(self, val)?;
        Ok(None)
    }
}

impl Access for PageRc {
    type Inner = Page;
    type Out = Self;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&Page) -> Result<Option<Page>>) -> Result<Option<Self>> {
        match update_inner(&**self)? {
            Some(val) => {
                PageRc::update(val, self, resolve)?;
                Ok(None)
            }
            None => Ok(None)
        }
    }
    fn replace(&self, update: &mut impl Updater, val: Page) -> Result<Option<Self>> {
        let rc = PageRc::update(val, self, update)?;
        Ok(None)
    }
}

struct Lens<'a, T, U, R, W> {
    _u: PhantomData<U>,
    r: R,
    w: W,
    t: &'a T
}
impl<'a, T, V, R, W> Access for Lens<'a, T, V, R, W>
where
    T: Clone,
    V: Access<Out=V>,
    R: Fn(&T) -> &V::Inner,
    W: Fn(&mut T) -> &mut V::Inner,
{
    type Inner = V::Inner;
    type Out = T;
    fn maybe_update<U: Resolve + Updater>(&self, resolve: &mut U, update_inner: impl FnOnce(&Self::Inner) -> pdf::error::Result<Option<Self::Inner>>) -> pdf::error::Result<Option<Self::Out>> {
        let inner: &V::Inner = (self.r)(&self.t);
        match update_inner(inner)? {
            None => Ok(None),
            Some(val) => {
                let mut copy = self.t.clone();
                *(self.w)(&mut copy) = val;
                Ok(Some(copy))
            }
        }
    }
    fn replace(&self, update: &mut impl Updater, val: Self::Inner) -> pdf::error::Result<Option<Self::Out>> {
        let mut copy = self.t.clone();
        *(self.w)(&mut copy) = val;
        Ok(Some(copy))
    }
}
impl<'a, T, V, R, W> Lens<'a, T, V, R, W> {
    fn new(t: &'a T, r: R, w: W) -> Self {
        Lens { _u: PhantomData, r, w, t }
    }
}

macro_rules! lens {
    ($base:ident . $field:ident) => {
        Lens::new($base, |base| &base.$field, |base| &mut base.$field)
    };
}
macro_rules! update {
    ($file:ident, $base:ident = $new:expr) => ({
        $base.replace(&mut $file, $new)
    });
    ($file:ident, $base:ident: [$ty:ty] $($path:tt)*) => ({
        let base: &$ty = $base;
        update!($file, base $($path)*)
    });
    ($file:ident, $base:ident ~ $($path:tt)*) => ({
        $base.maybe_update(&mut $file, |inner| update!($file, inner $($path)*))
    });
    ($file:ident, $base:ident . $field:ident $($path:tt)*) => ({
        let ref inner = $base.$field;
        match update!($file, inner $($path)*)? {
            Some(val) => {
                let mut base2 = Clone::clone($base);
                base2.$field = val;
                Ok(Some(base2))
            }
            None => Ok(None),
        }
    });
    ($file:ident, $base:ident [$idx:expr] $($path:tt)*) => ({
        let idx = $idx;
        let ref inner = $base[idx];
        match update!($file, inner $($path)*)? {
            None => Ok(None),
            Some(val) => {
                let mut base2 = $base.clone();
                base2[idx] = val;
                Ok(Some(base2))
            }
        }
    });
}


fn run() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);

    let mut file = FileOptions::cached().open(&path)?;
    let mut to_update_field: Option<_> = None;

    let font = Font::standard("Helvetica");
    let font_name = Name::from("Helvetica");
    let font = file.create(font)?;
    let mut fonts = HashMap::new();
    fonts.insert("Helvetica".into(), font.into());
    let resources = Resources {
        fonts,
        ..Default::default()
    };
    let resources = file.create(resources)?;

    let page0 = file.get_page(0).unwrap();
    let annots = page0
        .annotations
        .load(&file.resolver())
        .expect("can't load annotations");
    for (i, annot) in (*annots).iter().enumerate() {
        if let Some(ref a) = annot.appearance_streams {
            let normal = a.normal.data();
            match **normal {
                AppearanceStreamEntry::Single(ref s) => {
                    //dbg!(&s.stream.resources);

                    let form_dict = FormDict {
                        resources: Some(resources.clone().into()),
                        ..(**s.stream).clone()
                    };

                    let ops = vec![
                        Op::Save,
                        Op::TextFont {
                            name: font_name.clone(),
                            size: 14.0,
                        },
                        Op::TextDraw {
                            text: PdfString::from("Hello World!"),
                        },
                        Op::EndText,
                        Op::Restore,
                    ];
                    let stream = Stream::new(form_dict, &serialize_ops(&ops)?)?;

                    let form_xo = file.create(FormXObject { stream }).unwrap();
                    let normal2 = AppearanceStreamEntry::Single(form_xo);

                    let page = &page0;
                    update!(file, page.annotations ~ [i] ~ .appearance_streams: [Option::<MaybeRef<AppearanceStreams>>] ~ . normal = normal2);
                    page.tracer()
                }
                _ => {}
            }
        }
    }

    if let Some(ref forms) = file.get_root().forms {
        println!("Forms:");
        for field in forms.fields.iter().take(1) {
            print!("  {:?} = ", field.name);
            match field.value {
                Primitive::String(ref s) => println!("{}", s.to_string_lossy()),
                Primitive::Integer(i) =>    println!("{}", i),
                Primitive::Name(ref s) =>   println!("{}", s),
                ref p => println!("{:?}", p),
            }

            if to_update_field.is_none() {
                to_update_field = Some(field.clone());
            }
        }
    }

    if let Some(to_update_field) = to_update_field {
        println!("\nUpdating field:");
        println!("{:?}\n", to_update_field);

        let text = "Hello World!";
        let new_value: PdfString = PdfString::new(text.into());
        let mut updated_field = (*to_update_field).clone();
        updated_field.value = Primitive::String(new_value);

        //dbg!(&updated_field);

        let _reference = file.update(to_update_field.get_ref().get_inner(), updated_field)?;

        file.save_to("output/out.pdf")?;

        println!("\nUpdated field:");
        //println!("{:?}\n", reference);
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        println!("{e}");
    }
}
