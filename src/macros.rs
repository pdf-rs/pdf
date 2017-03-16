macro_rules! write_entry {
    ($out:expr, $key:tt, $val:expr) => {
        {
            $out.write(b"  ")?;
            $key.serialize($out)?;
            $out.write(b" ")?;
            $val.serialize($out)?;
            $out.write(b"\n")?;
        }
    }
}
macro_rules! write_entrys {
    ($out:expr, $key:tt << $val:expr $(,)*) => {
        write_entry!($out, $key, $val);
    };
    ($out:expr, $key:tt << $val:expr, $($rest:tt)*) => {
        {
            write_entry!($out, $key, $val);
            write_entrys!($out, $($rest)*);
        }
    };
    ($out:expr, $key:tt ? << $val:expr $(,)*) => {
        match &$val {
            &Some(ref v) => write_entry!($out, $key, v),
            &None => {}
        }
    };
    ($out:expr, $key:tt ? << $val:expr, $($rest:tt)*) => {
        {
            match &$val {
                &Some(ref v) => write_entry!($out, $key, v),
                &None => {}
            }
            write_entrys!($out, $($rest)*);
        }
    }
}

macro_rules! write_dict {
    ($out:expr, $($rest:tt)*) => {
        {
            write!($out, "<<\n")?;
            write_entrys!($out, $($rest)*);
            write!($out, ">>")?;
        }
    };
}


macro_rules! qtyped {
    (@field_ty $f_ty:ty, opt: false) => { $f_ty };
    (@field_ty $f_ty:ty, opt: true) => { Option<$f_ty> };

    (@field_serialize $self:expr, $out:expr, $name:ident, $key:tt, false) => {
        write!($out, concat!("/", stringify!($ty), " ")).unwrap();
        $self.$name.serialize($out);
        writeln!($out).unwrap();
    };

    (@field_serialize $self:expr, $out:expr, $name:ident, $key:tt, true) => {
        if let Some(f) = $self.$name.as_ref() {
            write!($out, concat!("/", stringify!($ty), " ")).unwrap();
            f.serialize($out);
            writeln!($out).unwrap();
        }
    };

    (
        @expand
        { ty_name: $ty_name:ident, },
        [ $({ name: $f_name:ident, ty: $f_ty:ident, key: $key:tt, opt: $f_opt:ident, })* ],
    ) => {
        pub struct $ty_name {
            $(
                #[doc = "PDF Key:"]
                #[doc = $key]
                $f_name: qtyped!(@field_ty $f_ty, opt: $f_opt),
            )*
        }

        impl PdfObject for $ty_name {
            fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
                writeln!(out, "<<")?;
                writeln!(out, "/Type {}", stringify!($ty_name))?;

                $(
                    qtyped!(@field_serialize self, out, $f_name, $key, $f_opt);
                )*

                writeln!(out, ">>")?;
                Ok(())
            }
        }
        impl $ty_name {
            fn from_dict(dict: &HashMap<String, &PdfObject>) -> $ty_name {
                $ty_name {
                    $(
                        $f_name: qtyped!(@dict_get dict, $key, $f_opt),
                    )*
                }
            }
            
            fn deserialize(lines: TODO) {
                // magic parser here ,..
            }
        }
    };
    
    (@dict_get $dict:expr, $f_ty:ident, $key:tt, false) => {
        (dict[concat!("/", stringify!($key))].downcast_ref()?).clone()
    };
    
    (@dict_get $dict:expr, $f_ty:ident, $key:tt, true) => {
        dict.get(concat!("/", stringify!($key))).downcast_ref().unwrap().clone()?
    };

    (
        @parse_body
        $prefix:tt,
        $fields:tt,
        $(,)*
    ) => {
        qtyped! {
            @expand
            $prefix,
            $fields,
        }
    };

    (
        @parse_body $prefix:tt, [$($fields:tt)*],
        $field_name:ident ($key:tt): Option<$field_ty:ident>,
        $($tail:tt)*
    ) => {
        qtyped! {
            @parse_body
            $prefix,
            [$($fields)* { name: $field_name, ty: $field_ty, key:$key, opt: true, }],
            $($tail)*
        }
    };

    (
        @parse_body $prefix:tt, [$($fields:tt)*],
        $field_name:ident: Option<$field_ty:ident>,
        $($tail:tt)*
    ) => {
        qtyped! {
            @parse_body
            $prefix,
            [$($fields)* { name: $field_name, ty: $field_ty, key:$field_ty, opt: true, }],
            $($tail)*
        }
    };
    
    (
        @parse_body $prefix:tt, [$($fields:tt)*],
        $field_name:ident ($key:tt): $field_ty:ident,
        $($tail:tt)*
    ) => {
        qtyped! {
            @parse_body $prefix,
            [$($fields)* { name: $field_name, ty: $field_ty, key:$key, opt: false, }],
            $($tail)*
        }
    };
    (
        @parse_body $prefix:tt, [$($fields:tt)*],
        $field_name:ident: $field_ty:ident,
        $($tail:tt)*
    ) => {
        qtyped! {
            @parse_body $prefix,
            [$($fields)* { name: $field_name, ty: $field_ty, key:$field_ty, opt: false, }],
            $($tail)*
        }
    };

    ($ty_name:ident { $($body:tt)* }) => {
        qtyped! {
            @parse_body
            { ty_name: $ty_name, },
            [],
            $($body)*,
        }
    };
}


