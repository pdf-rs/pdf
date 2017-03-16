use xref::XRefTable;

pub struct File<B> {
    backend:    B,
    refs:       XRefTable
}
