
fn build_xref_table() {
    warn!("can't read xref table: {:?}", e);
    let start_offset = t!(backend.locate_start_offset());
    let mut lexer = Lexer::new(t!(backend.read(..)));
    let mut objects = Vec::new();

    (|| -> Result<()> { loop {
        let offset = lexer.get_pos();
        let w1 = t!(lexer.next());
        let w2 = t!(lexer.next());
        let w3 = t!(lexer.next_expect("obj"));
        try_opt!(lexer.seek_substr("endobj"));

        objects.push((t!(w1.to::<ObjNr>()), t!(w2.to::<GenNr>()), offset));
    }})();

    objects.sort_unstable();
    let mut first_id = objects.first().map(|&(n, _, _)| n).unwrap_or(0);
    let mut last_id = objects.last().map(|&(n, _, _)| n).unwrap_or(0);
    let mut xref = XRefTable::new(1 + last_id - first_id);
    for &(obj_nr, gen_nr, offset) in objects.iter() {
        for n in first_id + 1 .. obj_nr {
            xref.push(XRef::Free { next_obj_nr: obj_nr, gen_nr: 0 });
        }
        if obj_nr == last_id {
            warn!("duplicate obj_nr {}", obj_nr);
            continue;
        }
        xref.push(XRef::Raw {
            pos: offset - start_offset,
            gen_nr
        });
        last_id = obj_nr;
    }

    return t!(Err(e));
}

fn build_catalog() {
    
}
