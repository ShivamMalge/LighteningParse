content = open('../lightningparse-core/tests/integration_real_pdfs.rs', 'r').read()

test_code = '''
#[test]
fn test_code_block_detection() {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push("code_block_fixture.pdf");
    let doc = lightningparse::parse_pdf_to_result(path.to_str().unwrap()).unwrap();
    let blocks = &doc.pages[0].blocks;
    
    // "This is a regular paragraph of body text." -> None
    assert_eq!(blocks[0].block_role().as_deref(), None);
    // "parse()" -> code
    assert_eq!(blocks[3].block_role().as_deref(), Some("code"));
    // " inline." -> None
    assert_eq!(blocks[4].block_role().as_deref(), None);
    // "def fibonacci(n):" -> code
    assert_eq!(blocks[5].block_role().as_deref(), Some("code"));
}
'''
if 'test_code_block_detection' not in content:
    content += test_code
open('../lightningparse-core/tests/integration_real_pdfs.rs', 'w').write(content)
