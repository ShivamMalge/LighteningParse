use lopdf::{Document, Object};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: dump_text_ops <pdf>");
        return;
    }
    let doc = Document::load(&args[1]).unwrap();
    for (page_num, page_id) in doc.get_pages() {
        if page_num != 1 { continue; }
        
        let content_data = doc.get_page_content(page_id).unwrap();
        let content = lopdf::content::Content::decode(&content_data).unwrap();
        
        for op in content.operations.iter() {
            if op.operator == "Tj" || op.operator == "TJ" || op.operator == "Tf" || op.operator == "BT" || op.operator == "ET" {
                println!("Op: {} {:?}", op.operator, op.operands);
            }
        }
    }
}
