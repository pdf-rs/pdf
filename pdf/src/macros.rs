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


