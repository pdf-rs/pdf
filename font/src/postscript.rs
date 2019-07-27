use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::borrow::{Cow};
use slotmap::SlotMap;
use tuple::{TupleElements, Map};
use decorum::R32;
use indexmap::set::IndexSet;
use crate::R;
use crate::parsers::{token, Token, comment, space};

new_key_type! {
    pub struct DictKey;
    pub struct ArrayKey;
    pub struct StringKey;
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct LitKey(usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum Item {
    Null,
    Bool(bool),
    Int(i32),
    Real(R32),
    Dict(DictKey),
    Array(ArrayKey),
    String(StringKey),
    Name(LitKey),
    Literal(LitKey),
    Operator(Operator),
    Mark,
    File
}

#[derive(Copy, Clone)]
pub struct RefDict<'a> {
    vm: &'a Vm,
    dict: &'a Dictionary
}
impl<'a> RefDict<'a> {
    pub fn iter(&self) -> impl Iterator<Item=(RefItem<'a>, RefItem<'a>)> {
        let vm = self.vm;
        self.dict.iter().map(move |(k, v)| (RefItem::new(vm, *k), RefItem::new(vm, *v)))
    }
    pub fn get(&self, key: &str) -> Option<RefItem<'a>> {
        self.vm.literals.get_full(key.as_bytes())
            .and_then(|(index, _)| self.dict.get(&Item::Literal(LitKey(index))))
            .map(|&item| RefItem::new(self.vm, item))
    }
    pub fn len(&self) -> usize {
        self.dict.len()
    }
}
impl<'a> fmt::Debug for RefDict<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[derive(Copy, Clone)]
pub struct RefArray<'a> {
    vm: &'a Vm,
    array: &'a Array
}
impl<'a> RefArray<'a> {
    pub fn iter(&self) -> impl Iterator<Item=RefItem<'a>> {
        let vm = self.vm;
        self.array.iter()
            .map(move |&item| (RefItem::new(vm, item)))
    }
    pub fn get(&self, index: usize) -> Option<RefItem<'a>> {
        self.array.get(index)
            .map(|&item| RefItem::new(self.vm, item))
    }
    pub fn len(&self) -> usize {
        self.array.len()
    }
}
impl<'a> fmt::Debug for RefArray<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

pub enum RefItem<'a> {
    Null,
    Bool(bool),
    Int(i32),
    Real(f32),
    Dict(RefDict<'a>),
    Array(RefArray<'a>),
    String(&'a [u8]),
    Name(&'a [u8]),
    Literal(&'a [u8]),
    Operator(Operator),
    Mark,
    File
}
fn print_string(s: &[u8]) -> Cow<str> {
    String::from_utf8_lossy(&s[.. s.len().min(100)])
}
impl<'a> RefItem<'a> {
    fn new(vm: &'a Vm, item: Item) -> RefItem<'a> {
        match item {
            Item::Null => RefItem::Null,
            Item::Bool(b) => RefItem::Bool(b),
            Item::Int(i) => RefItem::Int(i),
            Item::Real(r) => RefItem::Real(r.into()),
            Item::Dict(key) => RefItem::Dict(RefDict { vm, dict: vm.get_dict(key) }),
            Item::Array(key) => RefItem::Array(RefArray { vm, array: vm.get_array(key) }),
            Item::String(key) => RefItem::String(vm.get_string(key)),
            Item::Name(key) => RefItem::Name(vm.get_lit(key)),
            Item::Literal(key) => RefItem::Literal(vm.get_lit(key)),
            Item::Operator(op) => RefItem::Operator(op),
            Item::Mark => RefItem::Mark,
            Item::File => RefItem::File
        }
    }
    pub fn as_dict(&self) -> Option<RefDict<'a>> {
        match *self {
            RefItem::Dict(dict) => Some(dict),
            _ => None
        }
    }
    pub fn as_array(&self) -> Option<RefArray<'a>> {
        match *self {
            RefItem::Array(array) => Some(array),
            _ => None
        }
    }
    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match *self {
            RefItem::String(bytes) |
            RefItem::Name(bytes) |
            RefItem::Literal(bytes) => Some(bytes),
            _ => None
        }
    }
    pub fn as_str(&self) -> Option<&'a str> {
        self.as_bytes().map(|b| std::str::from_utf8(b).unwrap())
    }
    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            RefItem::Int(i) => Some(i as f32),
            RefItem::Real(r) => Some(r.into()),
            _ => None
        }
    }
    pub fn as_int(&self) -> Option<i32> {
        match *self {
            RefItem::Int(i) => Some(i),
            _ => None
        }
    }
    
}

impl<'a> fmt::Debug for RefItem<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefItem::Null => write!(f, "Null"),
            RefItem::Mark => write!(f, "Mark"),
            RefItem::File => write!(f, "File"),
            RefItem::Operator(op) => op.fmt(f),
            RefItem::Bool(b) => b.fmt(f),
            RefItem::Int(i) => i.fmt(f),
            RefItem::Real(r) => r.fmt(f),
            RefItem::Dict(dict) => dict.fmt(f),
            RefItem::Array(array) => array.fmt(f),
            RefItem::String(s) => write!(f, "({:?})", print_string(s)),
            RefItem::Literal(s) => write!(f, "/{:?}", print_string(s)),
            RefItem::Name(s) => write!(f, "{:?}", print_string(s)),
        }
    }
}

type Array = Vec<Item>;
type Dictionary = HashMap<Item, Item>;

#[derive(Debug)]
struct Mode {
    write: bool,
    execute: bool,
    read: bool
}
impl Mode {
    fn all() -> Mode {
        Mode {
            write: true,
            execute: true,
            read: true
        }
    }
    fn read_only(&mut self) {
        self.write = false;
    }
    fn execute_only(&mut self) {
        self.read = false;
        self.write = false;
    }
    fn noaccess(&mut self) {
        self.write = false;
        self.execute = false;
        self.read = false;
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Operator {
    Array,
    Begin,
    ClearToMark,
    CloseFile,
    CurrentDict,
    CurrentFile,
    DefineFont,
    For,
    Def,
    Dict,
    Dup,
    End,
    EndArray,
    Exch,
    ExecuteOnly,
    Eexec,
    False,
    Get,
    Index,
    Mark,
    NoAccess,
    Pop,
    Put,
    ReadOnly,
    ReadString,
    String,
    True,
}

macro_rules! map {
    ($($key:expr => $val:expr),*) => (
        [$(($key, $val)),*]
    )
}

const OPERATOR_MAP: &[(&'static str, Operator)] = {
    use Operator::*;
    &map!{
        "array"         => Array,
        "begin"         => Begin,
        "currentdict"   => CurrentDict,
        "currentfile"   => CurrentFile,
        "cleartomark"   => ClearToMark,
        "closefile"     => CloseFile,
        "definefont"    => DefineFont,
        "for"           => For,
        "def"           => Def,
        "dict"          => Dict,
        "dup"           => Dup,
        "end"           => End,
        "exch"          => Exch,
        "executeonly"   => ExecuteOnly,
        "eexec"         => Eexec,
        "false"         => False,
        "get"           => Get,
        "index"         => Index,
        "mark"          => Mark,
        "noaccess"      => NoAccess,
        "pop"           => Pop,
        "put"           => Put,
        "readonly"      => ReadOnly,
        "readstring"    => ReadString,
        "string"        => String,
        "true"          => True,
        "]"             => EndArray,
        "["             => Mark
    }
};

pub struct Input<'a> {
    data:   &'a [u8]
}
impl<'a> Input<'a> {
    pub fn new(data: &'a [u8]) -> Input<'a> {
        Input { data }
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    fn take(&mut self, n: usize) -> &'a [u8] {
        let (first, second) = self.data.split_at(n);
        self.data = second;
        first
    }
    fn advance(&mut self, n: usize) {
        self.data = &self.data[n ..];
    }
    // true if buf.len() bytes were read
    // false if EOF (buf will be truncated)
    fn read_to(&mut self, buf: &mut Vec<u8>) -> bool {
        if self.len() >= buf.len() {
            let len = buf.len();
            // normal case 
            buf.copy_from_slice(self.take(len));
            true
        } else {
            let len = self.len();
            buf.truncate(len);
            buf.copy_from_slice(self.take(len));
            false
        }
    }
    fn try_parse<T>(&mut self, parser: impl Fn(&'a [u8]) -> R<'a, T>) -> Option<T> {
        match parser(self.data) {
            Ok((i, t)) => {
                let n = self.data.len() - i.len();
                self.advance(n);
                Some(t)
            },
            Err(_) => None
        }
    }
    fn parse<T>(&mut self, parser: impl Fn(&'a [u8]) -> R<'a, T>) -> T {
        self.try_parse(parser).unwrap()
    }
}
impl<'a> Deref for Input<'a> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.data
    }
}
pub struct Vm {
    dicts:      SlotMap<DictKey, (Dictionary, Mode)>,
    arrays:     SlotMap<ArrayKey, (Array, Mode)>,
    strings:    SlotMap<StringKey, (Vec<u8>, Mode)>,
    literals:   IndexSet<Vec<u8>>,
    fonts:      HashMap<String, DictKey>,
    dict_stack: Vec<DictKey>,
    stack:      Vec<Item>
}
impl Vm {
    pub fn new() -> Vm {
        let mut vm = Vm { 
            dicts: SlotMap::with_key(),
            arrays: SlotMap::with_key(),
            strings: SlotMap::with_key(),
            literals: IndexSet::new(),
            fonts: HashMap::new(),
            dict_stack: Vec::new(),
            stack: Vec::new(),
        };
        let system_dict = OPERATOR_MAP.iter()
            .map(|&(name, op)| (Item::Literal(vm.make_lit(name.as_bytes())), Item::Operator(op))) 
            .collect();
        
        let key = vm.make_dict(system_dict, Mode { write: false, execute: false, read: true });
        vm.push_dict(key);
        
        let key = vm.make_dict(Dictionary::new(), Mode { write: true, execute: false, read: true });
        vm.push_dict(key);
        
        vm
    }
    pub fn fonts<'a>(&'a self) -> impl Iterator<Item=(&'a str, RefDict<'a>)> {
        self.fonts.iter().map(move |(key, &dict)| (
            key.as_str(),
            RefDict { vm: self, dict: self.get_dict(dict) }
        ))
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
    fn make_lit(&mut self, lit: &[u8]) -> LitKey {
        if let Some((index, _)) = self.literals.get_full(lit) {
            return LitKey(index);
        }
        let (index, _) = self.literals.insert_full(lit.into());
        LitKey(index)
    }
    fn get_lit(&self, LitKey(index): LitKey) -> &[u8] {
        self.literals.get_index(index).expect("no such key").as_slice()
    }
    fn make_array(&mut self, array: Array) -> ArrayKey {
        self.arrays.insert((array, Mode::all()))
    }
    fn make_string(&mut self, s: Vec<u8>) -> StringKey {
        self.strings.insert((s, Mode::all()))
    }
    fn make_dict(&mut self, dict: Dictionary, mode: Mode) -> DictKey {
        self.dicts.insert((dict, mode))
    }
    fn get_string(&self, key: StringKey) -> &[u8] {
        &self.strings.get(key).unwrap().0
    }
    fn get_string_mut(&mut self, key: StringKey) -> &mut Vec<u8> {
        &mut self.strings.get_mut(key).unwrap().0
    }
    fn get_array(&self, key: ArrayKey) -> &Array {
        match self.arrays.get(key).expect("no item for key") {
            (ref array, _) => array
        }
    }
    fn get_array_mut(&mut self, key: ArrayKey) -> &mut Array {
        match self.arrays.get_mut(key).expect("no item for key") {
            (ref mut array, Mode { write: true, .. }) => array,
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
            (ref mut dict, Mode { write: true, .. }) => dict,
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
    
    // resolve name items. or keep them unchanged if unresolved
    fn resolve(&self, item: Item) -> Option<Item> {
        for &dict_key in self.dict_stack.iter().rev() {
            let dict = self.get_dict(dict_key);
            if let Some(&val) = dict.get(&item) {
                return Some(val.clone());
            }
        }
        None
    }
    
    fn transform_token(&mut self, token: Token) -> Item {
        match token {
            Token::Int(i) => Item::Int(i),
            Token::Real(r) => Item::Real(r),
            Token::Literal(name) => Item::Literal(self.make_lit(name)),
            Token::Name(name) => Item::Name(self.make_lit(name)),
            Token::String(vec) => Item::String(self.make_string(vec)),
            Token::Procedure(tokens) => {
                let array = tokens.into_iter().map(|t| self.transform_token(t)).collect();
                Item::Array(self.make_array(array))
            }
        }
    }
    pub fn exec_token(&mut self, token: Token, input: &mut Input) {
        let item = self.transform_token(token);
        match item {
            Item::Operator(op) => self.exec_operator(op, input),
            Item::Name(key) => {
                let item = self.resolve(Item::Literal(key)).expect("undefined");
                self.exec(item, input)
            }
            item => self.push(item)
        }
    }
    
    fn exec(&mut self, item: Item, input: &mut Input) {
        debug!("exec {:?}", self.display(item));
        match item {
            Item::Operator(op) => self.exec_operator(op, input),
            Item::Name(key) => {
                let item = self.resolve(Item::Literal(key)).expect("undefined");
                self.exec(item, input)
            }
            Item::Array(key) => {
                // check that the array is executable
                let mut pos = 0;
                loop {
                    match self.arrays.get(key).expect("no item for key") {
                        (ref items, Mode { execute: true, .. }) => {
                            match items.get(pos) {
                                Some(&item) => self.exec(item, input),
                                None => break
                            }
                        },
                        _ => panic!("exec: array is not executable")
                    }
                    pos += 1;
                }
            }
            item => self.push(item)
        }
    }
    
    #[deny(unreachable_patterns)]
    fn exec_operator(&mut self, op: Operator, input: &mut Input) {
        match op {
            Operator::Array => {
                match self.pop() {
                    Item::Int(i) if i >= 0 => {
                        let key = self.make_array(vec![Item::Null; i as usize]);
                        self.push(Item::Array(key));
                    }
                    i => panic!("array: invalid count: {:?}", self.display(i))
                }
            }
            Operator::Begin => {
                match self.pop() {
                    Item::Dict(dict) => self.push_dict(dict),
                    item => panic!("begin: unespected item {:?}", self.display(item))
                }
            }
            Operator::CurrentDict => {
                let &key = self.dict_stack.last().expect("no current dictionary");
                self.push(Item::Dict(key));
            }
            Operator::DefineFont => {
                match self.pop_tuple() {
                    (Item::Literal(lit), Item::Dict(dict_key)) => {
                        let font_name = String::from_utf8(self.get_lit(lit).to_owned())
                            .expect("Font name is not valid UTF-8");
                        let (ref mut dict, ref mut mode) = self.dicts.get_mut(dict_key).unwrap();
                        mode.read_only();
                        self.fonts.insert(font_name, dict_key);
                        self.push(Item::Dict(dict_key));
                    }
                    args => panic!("definefont: invalid args {:?}", self.display_tuple(args))
                }
            }
            Operator::For => {
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
                                self.exec(item.clone(), input);
                            }
                            val += increment;
                        }
                    },
                    args => panic!("for: invalid args {:?}", self.display_tuple(args))
                }
            }
            Operator::Def => {
                let (key, val) = self.pop_tuple();
                self.current_dict_mut().insert(key, val);
            }
            Operator::Dict => {
                match self.pop() {
                    Item::Int(n) if n >= 0 => {
                        let dict = self.make_dict(Dictionary::with_capacity(n as usize), Mode::all());
                        self.push(Item::Dict(dict));
                    }
                    arg => panic!("dict: unsupported {:?}", self.display(arg))
                }
            }
            Operator::String => {
                match self.pop() {
                    Item::Int(n) if n >= 0 => {
                        let string = self.make_string(vec![0; n as usize]);
                        self.push(Item::String(string));
                    },
                    len => panic!("string: unsupported {:?}", self.display(len))
                }
            },
            Operator::ReadString => {
                match self.pop_tuple() {
                    (Item::File, Item::String(key)) => {
                        let string = self.get_string_mut(key);
                        let flag = input.read_to(string);
                        input.parse(space);
                        
                        self.push(Item::String(key));
                        self.push(Item::Bool(flag));
                    },
                    args => panic!("readstring: invalid arguments {:?}", self.display_tuple(args))
                }
            }
            Operator::Dup => {
                let v = self.pop();
                self.push(v.clone());
                self.push(v);
            },
            Operator::Pop => {
                self.pop();
            }
            Operator::End => self.pop_dict(),
            Operator::Exch => {
                let (a, b) = self.pop_tuple();
                self.push(b);
                self.push(a);
            }
            Operator::False => self.push(Item::Bool(false)),
            Operator::True => self.push(Item::Bool(true)),
            Operator::Index => match self.pop() {
                Item::Int(idx) if idx >= 0 => {
                    let n = self.stack.len();
                    let item = self.stack.get(n - idx as usize - 1).expect("out of bounds").clone();
                    self.push(item);
                },
                arg => panic!("index: invalid argument {:?}", self.display(arg))
            }
            Operator::Get => match self.pop_tuple() {
                (Item::Array(key), Item::Int(index)) if index >= 0 => {
                    let &item = self.get_array(key).get(index as usize).expect("out of bounds");
                    self.push(item);
                }
                (Item::String(key), Item::Int(index)) if index >= 0 => {
                    let &byte = self.get_string(key).get(index as usize).expect("out of bounds");
                    self.push(Item::Int(byte as i32));
                }
                (Item::Dict(dict_key), key) => {
                    let &item = self.get_dict(dict_key).get(&key).expect("no such entry");
                    self.push(item);
                }
                args => panic!("get: invalid arguments {:?}", self.display_tuple(args))
            }
            Operator::Put => {
                let (a, b, c) = self.pop_tuple();
                let a = self.resolve(a).unwrap_or(a);
                match (a, b, c) {
                    (Item::Array(array), Item::Int(idx), any) => {
                        *self.get_array_mut(array).get_mut(idx as usize).expect("out of bounds") = any;
                    }
                    (Item::Dict(dict), key, any) => {
                        self.get_dict_mut(dict).insert(key, any);
                    }
                    args => panic!("put: unsupported args {:?})", self.display_tuple(args))
                }
            }
            Operator::ReadOnly => {
                let item = self.pop();
                match item {
                    Item::Array(key) => self.arrays[key].1.read_only(),
                    Item::Dict(key) => self.dicts[key].1.read_only(),
                    Item::String(key) => self.strings[key].1.read_only(),
                    i => panic!("can't make {:?} readonly", self.display(i))
                }
                self.push(item);
            },
            Operator::ExecuteOnly => {
                let item = self.pop();
                match item {
                    Item::Array(key) => self.arrays[key].1.execute_only(),
                    Item::Dict(key) => self.dicts[key].1.execute_only(),
                    Item::String(key) => self.strings[key].1.execute_only(),
                    i => panic!("can't make {:?} executeonly", self.display(i))
                }
                self.push(item);
            },
            Operator::NoAccess => {
                let item = self.pop();
                match item {
                    Item::Array(key) => self.arrays[key].1.noaccess(),
                    Item::Dict(key) => self.dicts[key].1.noaccess(),
                    Item::String(key) => self.strings[key].1.noaccess(),
                    i => panic!("can't make {:?} executeonly", self.display(i))
                }
                self.push(item);
            }
            Operator::EndArray => {
                let start = self.stack.iter()
                    .rposition(|item| *item == Item::Mark)
                    .expect("unmatched ]");
                let array = self.stack.drain(start ..).skip(1).collect(); // skip the Mark
                let key = self.make_array(array);
                self.push(Item::Array(key));
            },
            Operator::Mark => self.push(Item::Mark),
            Operator::ClearToMark => {
                let start = self.stack.iter()
                    .rposition(|item| *item == Item::Mark)
                    .expect("unmatched mark");
                self.stack.drain(start ..);
            }
            Operator::CurrentFile => self.push(Item::File),
            Operator::CloseFile => {
                match self.pop() {
                    Item::File => {},
                    arg => panic!("closefile: invalid arg {:?})", self.display(arg))
                }
            }
            Operator::Eexec => {
                match self.pop() {
                    Item::File => {},
                    Item::String(key) => {
                        unimplemented!()
                        // let mut input = Input::new(self.get_string(key));
                        // self.parse_and_exec(&mut input);
                    },
                    arg => panic!("eexec: unsupported arg {:?})", self.display(arg))
                }
            }
        }
    }
    pub fn display(&self, item: Item) -> RefItem {
        RefItem::new(self, item)
    }
    pub fn display_tuple<'a, T>(&'a self, tuple: T) -> T::Output where
        T: TupleElements<Element=Item>,
        T: Map<RefItem<'a>>
    {
        tuple.map(|item| RefItem::new(self, item))
    }
    pub fn print_stack(&self) {
        for (i, &item) in self.stack.iter().rev().enumerate() {
            println!("stack[{}]: {:?}", i, self.display(item));
        }
    }
    pub fn step(&mut self, input: &mut Input) {
        if let Some(_) = input.try_parse(comment) {
            return;
        }
        let tk = input.parse(token);
        
        debug!("token: {:?}", tk);
        self.exec_token(tk, input);
    }
    pub fn parse_and_exec(&mut self, data: &[u8]) {
        let mut input = Input::new(data);
        while input.len() > 0 {
            self.step(&mut input);
        }
    }
}

