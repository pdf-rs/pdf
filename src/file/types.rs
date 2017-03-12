/*
/// `/Type Page`
qtyped!(Page {
    parent: PageTree,
    resources: Option<Resources>,
});
/// `/Type Pages`
qtyped!(PageTree {
    parent: Option<PageTree>,
    kids: Vec<ObjectId>,
    count: i32,
    resources: Option<Resources>,
});
/// `/Type Resources` - resource dictionary.
qtyped!(Resources {
    ext_g_state: Option<ExtGState>,
    color_space: Dictionary,
    // TODO:
    // Pattern
    // Shading
    // XObject
    // Font
    // ProcSet
    // Properties

});

/// `/Type ExtGState` - graphics state parameter dictionary.
qtyped!(ExtGState {
    line_width: Option<String>,
    line_cap_style: Option<i32>,
    line_join_style: Option<i32>,
    // TODO ETC
});

/// `/Type Catalog`
pub struct Catalog {
    pub version: Option<String>,
    /// `/Pages`
    pub page_tree: PageTree,
    // TODO PageLabels
    pub names: Option<Dictionary>,
    // TODO rest
}
 */
