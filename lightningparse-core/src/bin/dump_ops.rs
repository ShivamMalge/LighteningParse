use lopdf::Document;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: dump_ops <pdf>");
        return;
    }
    let doc = Document::load(&args[1]).unwrap();
    for (page_num, page_id) in doc.get_pages() {
        if page_num != 1 { continue; }
        
        let content_data = doc.get_page_content(page_id).unwrap();
        let content = lopdf::content::Content::decode(&content_data).unwrap();
        
        for op in content.operations.iter().take(20) {
            println!("Op: {} {:?}", op.operator, op.operands);
        }
        println!("... and more");
    }
}
