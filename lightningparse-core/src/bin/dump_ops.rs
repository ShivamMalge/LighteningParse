fn main() {
    let doc = lopdf::Document::load("../lightningparse-core/tests/fixtures/code_block_fixture.pdf").unwrap();
    let content_data = doc.get_page_content(doc.get_pages()[&1]).unwrap();
    let content = lopdf::content::Content::decode(&content_data).unwrap();
    for op in content.operations {
        println!("Op: {} {:?}", op.operator, op.operands);
    }
}
