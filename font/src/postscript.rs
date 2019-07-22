use std::collections::HashMap;
use std::fmt;
use nom::{
    bytes::complete::{tag, take_while},
    error::{make_error, ErrorKind},
    Err::Failure
};
use slotmap::SlotMap;
use tuple::TupleElements;
use decorum::R32;
use istring::IString;
use crate::R;
use crate::parsers::*;


new_key_type! {
    pub struct DictKey;
    pub struct ArrayKey;
    pub struct StringKey;
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Item {
    Null,
    Bool(bool),
    Int(i32),
    Real(R32),
    Dict(DictKey),
    Array(ArrayKey),
    Literal(StringKey),
    Name(IString),
}
type Array = Vec<Item>;
type Dictionary = HashMap<Item, Item>;

#[derive(Debug)]
struct Mode {
    writable: bool,
    executable: bool,
}
impl Mode {
    fn all() -> Mode {
        Mode {
            writable: true,
            executable: true
        }
    }
    fn read_only(&mut self) {
        self.writable = false;
    }
    fn execute_only(&mut self) {
        self.executable = false
    }
}
enum FileSource {
    Transparent,
    ExecEncrypted
}

#[derive(Debug)]
pub struct Vm {
    dicts:  SlotMap<DictKey, (Dictionary, Mode)>,
    arrays: SlotMap<ArrayKey, (Array, Mode)>,
    strings: SlotMap<StringKey, (Vec<u8>, Mode)>,
    dict_stack: Vec<DictKey>,
    stack:  Vec<Item>
}
impl Vm {
    pub fn new() -> Vm {
        Vm { 
            dicts: SlotMap::with_key(),
            arrays: SlotMap::with_key(),
            strings: SlotMap::with_key(),
            dict_stack: Vec::new(),
            stack: Vec::new()
        }
    }
    fn pop_tuple<T>(&mut self) -> T where
        T: TupleElements<Element=Item>
    {
        let range = self.stack.len() - T::N ..;
        T::from_iter(self.stack.drain(range)).unwrap()
    }
    fn pop(&mut self) -> Item {
        self.stack.pop().expect("empty stack")
    }
    fn push(&mut self, item: Item) {
        self.stack.push(item);
    }
    fn push_dict(&mut self, dict: DictKey) {
        self.dict_stack.push(dict);
    }
    fn make_array(&mut self, array: Array) -> ArrayKey {
        self.arrays.insert((array, Mode::all()))
    }
    fn make_string(&mut self, s: Vec<u8>) -> StringKey {
        println!("{:?}", std::str::from_utf8(&s[.. s.len().min(100)]));
        assert!(s.len() < 100);
        self.strings.insert((s, Mode::all()))
    }
    fn make_dict(&mut self) -> DictKey {
        self.dicts.insert((Dictionary::new(), Mode::all()))
    }
    fn get_string(&self, key: StringKey) -> &[u8] {
        &self.strings.get(key).unwrap().0
    }
    fn get_array(&self, key: ArrayKey) -> &Array {
        match self.arrays.get(key).expect("no item for key") {
            (ref array, _) => array
        }
    }
    fn get_array_mut(&mut self, key: ArrayKey) -> &mut Array {
        match self.arrays.get_mut(key).expect("no item for key") {
            (ref mut array, Mode { writable: true, .. }) => array,
            _ => panic!("array is locked")
        }
    }
    fn get_dict(&self, key: DictKey) -> &Dictionary {
        match self.dicts.get(key).expect("no item for key") {
            (ref dict, _) => dict
        }
    }
    fn get_dict_mut(&mut self, key: DictKey) -> &mut Dictionary {
        match self.dicts.get_mut(key).expect("no item for key") {
            (ref mut dict, Mode { writable: true, .. }) => dict,
            _ => panic!("dict is locked")
        }
    }
    fn pop_dict(&mut self) {
        self.dict_stack.pop();
    }
    fn current_dict(&self) -> &Dictionary {
        let &key = self.dict_stack.last().expect("no current dict");
        self.get_dict(key)
    }
    fn current_dict_mut(&mut self) -> &mut Dictionary {
        let &key = self.dict_stack.last().expect("no current dict");
        self.get_dict_mut(key)
    }
    pub fn stack(&self) -> &[Item] {
        &self.stack
    }
    pub fn exec(&mut self, item: Item) {
        debug!("exec {:?}", self.display(&item));
        match item {
            Item::Name(ref name) => match name.as_str() {
                "array" => {
                    match self.pop() {
                        Item::Int(i) if i >= 0 => {
                            let key = self.make_array(vec![Item::Null; i as usize]);
                            self.push(Item::Array(key));
                        }
                        i => panic!("array: invalid count: {:?}", i)
                    }
                }
                "begin" => {
                    match self.pop() {
                        Item::Dict(dict) => self.push_dict(dict),
                        item => panic!("begin: unespected item {:?}", item)
                    }
                }
                "currentdict" => {
                    let &key = self.dict_stack.last().expect("no current dictionary");
                    self.push(Item::Dict(key));
                }
                "for" => {
                    match self.pop_tuple() {
                        (Item::Int(initial), Item::Int(increment), Item::Int(limit), Item::Array(procedure)) => {
                            match increment {
                                i if i > 0 => assert!(limit > initial),
                                i if i < 0 => assert!(limit < initial),
                                _ => panic!("zero increment")
                            }
                            // proc would be allowed to modify the procedure array…
                            let proc_array = self.get_array(procedure).clone();
                            let mut val = initial;
                            while val < limit {
                                self.push(Item::Int(val));
                                for item in &proc_array {
                                    self.exec(item.clone());
                                }
                                val += increment;
                            }
                        },
                        args => panic!("for: invalid args {:?}", args)
                    }
                }
                "def" => {
                    let (key, val) = self.pop_tuple();
                    self.current_dict_mut().insert(key, val);
                }
                "dict" => {
                    let dict = self.make_dict();
                    self.push(Item::Dict(dict));
                }
                "dup" => {
                    let v = self.pop();
                    self.push(v.clone());
                    self.push(v);
                }
                "end" => self.pop_dict(),
                "exch" => {
                    let (a, b) = self.pop_tuple();
                    self.push(b);
                    self.push(a);
                }
                "executeonly" => {
                    let item = self.pop();
                    match item {
                        Item::Array(key) => self.arrays[key].1.read_only(),
                        Item::Dict(key) => self.dicts[key].1.read_only(),
                        Item::Literal(key) => self.strings[key].1.read_only(),
                        ref i => panic!("can't make {:?} readonly", i)
                    }
                    self.push(item);
                },
                "false" => self.push(Item::Bool(false)),
                "index" => match self.pop() {
                    Item::Int(idx) if idx >= 0 => {
                        let n = self.stack.len();
                        let item = self.stack.get(n - idx as usize - 1).expect("out of bounds").clone();
                        self.push(item);
                    },
                    arg => panic!("index: invalid argument {:?}", arg)
                }
                "put" => {
                    let (a, b, c) = self.pop_tuple();
                    match (a, b, c) {
                        (Item::Array(array), Item::Int(idx), any) => {
                            *self.get_array_mut(array).get_mut(idx as usize).expect("out of bounds") = any;
                        }
                        (Item::Dict(dict), key, any) => {
                            self.get_dict_mut(dict).insert(key, any);
                        }
                        (a, b, c) => panic!("put: unsupported args {:?}, {:?}, {:?})", a, b, c)
                    }
                }
                "readonly" => {
                    let item = self.pop();
                    match item {
                        Item::Array(key) => self.arrays[key].1.read_only(),
                        Item::Dict(key) => self.dicts[key].1.read_only(),
                        Item::Literal(key) => self.strings[key].1.read_only(),
                        ref i => panic!("can't make {:?} readonly", i)
                    }
                    self.push(item);
                },
                "true" => self.push(Item::Bool(true)),
                "]" => {
                    let start = self.stack.iter().rposition(|item| {
                        match item {
                            Item::Name(ref name) => name == "[",
                            _ => false
                        }
                    }).expect("unmatched ]");
                    let array = self.stack.drain(start ..).collect();
                    let key = self.make_array(array);
                    self.push(Item::Array(key));
                },
                "[" => self.push(item),
                name => panic!("unknown name: {}", name)
            },
            _ => self.push(item)
        }
    }
    pub fn parse<'a>(&mut self, i: &'a [u8]) -> R<'a, Item> {
        if let Ok((i, j)) = integer(i) {
            return Ok((i, Item::Int(j)))
        }
        if let Ok((i, f)) = float(i) {
            return Ok((i, Item::Real(f.into())))
        }
        if let Ok((i, lit)) = literal(i) {
            return Ok((i, Item::Literal(self.make_string(lit))));
        }
        if let Ok((i, array)) = procedure(self, i) {
            return Ok((i, Item::Array(self.make_array(array))));
        }
        if let Ok((i, b)) = name(i) {
            let s = std::str::from_utf8(b).unwrap();
            return Ok((i, Item::Name(s.into())));
        }
        Err(Failure(make_error(i, ErrorKind::Alt)))
    }
    pub fn display<'a>(&'a self, item: &'a Item) -> DisplayItem<'a> {
        DisplayItem(self, item)
    }
    pub fn print_stack(&self) {
        for (i, item) in self.stack.iter().rev().enumerate() {
            println!("stack[{}]: {:?}", i, self.display(item));
        }
    }
}

pub struct DisplayItem<'a>(&'a Vm, &'a Item);

impl<'a> fmt::Debug for DisplayItem<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.1 {
            Item::Null => write!(f, "null"),
            Item::Bool(b) => b.fmt(f),
            Item::Int(i) => i.fmt(f),
            Item::Real(r) => r.fmt(f),
            Item::Dict(key) => f.debug_map()
                .entries(
                    self.0.get_dict(key).iter()
                    .map(|(key, val)| (DisplayItem(self.0, key), DisplayItem(self.0, val)))
                )
                .finish(),
            Item::Array(key) => f.debug_list()
                .entries(
                    self.0.get_array(key).iter()
                    .map(|item| DisplayItem(self.0, item))
                ).finish(),
            Item::Literal(key) => String::from_utf8_lossy(self.0.get_string(key)).fmt(f),
            Item::Name(ref s) => s.fmt(f)
        }
    }
}

fn procedure<'a>(vm: &mut Vm, i: &'a [u8]) -> R<'a, Vec<Item>> {
    let (i, _) = tag("{")(i)?;
    let (i, _) = take_while(word_sep)(i)?;
    
    let mut array = Vec::new();
    let mut input = i;
    
    while let Ok((i, item)) = vm.parse(input) {
        array.push(item);
        let (i, _) = take_while(word_sep)(i)?;
        input = i;
    }
    let (i, _) = tag("}")(input)?;
    
    Ok((i, array))
}
