import json
from typing import List, Dict, Any, Optional
from langchain_core.documents import Document

class MetadataAwareChunker:
    """
    Consumes the structured JSON from lightningparse-core and chunks it.
    Respects `section_id` boundaries (e.g. headers) and carries `page_num` into chunk metadata.
    """
    
    def __init__(self, max_chars_per_chunk: int = 1500):
        self.max_chars_per_chunk = max_chars_per_chunk

    def chunk(self, parsed_json: str) -> List[Document]:
        """
        Takes raw JSON output from the Rust core and returns LangChain Documents.
        """
        data = json.loads(parsed_json)
        documents = []
        
        for page in data.get("pages", []):
            page_num = page.get("page_num", 0)
            blocks = page.get("blocks", [])
            
            current_chunk_text = []
            current_chunk_chars = 0
            current_chunk_section_ids = set()
            
            for block in blocks:
                section_id = block.get("section_id", "body")
                
                # Exclude boilerplate from retrieval chunks
                if section_id in ("header", "footer", "footnote"):
                    continue
                    
                is_table = block.get("type") == "table"
                
                if is_table:
                    # Serialize table rows to Markdown
                    rows = block.get("rows", [])
                    if not rows:
                        continue
                    # First row is treated as header (or just standard Markdown format)
                    md_rows = []
                    for r in rows:
                        md_rows.append("| " + " | ".join(str(cell).replace('\n', ' ') for cell in r) + " |")
                    
                    if len(md_rows) > 0:
                        # Add a separator after the first row to make it valid Markdown
                        col_count = len(rows[0])
                        separator = "| " + " | ".join(["---"] * col_count) + " |"
                        md_rows.insert(1, separator)
                        
                    text = "\n".join(md_rows)
                    source = block.get("source", "digital")
                else:
                    text = block.get("text", "").strip()
                    source = block.get("source", "digital")
                
                if not text:
                    continue
                    
                # If we hit a header/title, or if the chunk is too large, we break the chunk
                is_boundary = section_id in ("title",)
                is_too_large = (current_chunk_chars + len(text) > self.max_chars_per_chunk)
                
                if (is_boundary or is_too_large) and current_chunk_text:
                    # Flush the current chunk
                    documents.append(
                        Document(
                            page_content="\n".join(current_chunk_text),
                            metadata={
                                "page_num": page_num,
                                "source_type": source,
                                "section_ids": list(current_chunk_section_ids)
                            }
                        )
                    )
                    current_chunk_text = []
                    current_chunk_chars = 0
                    current_chunk_section_ids = set()
                    
                # Add current block to chunk
                current_chunk_text.append(text)
                current_chunk_chars += len(text)
                current_chunk_section_ids.add(section_id)
                
            # Flush remaining text for the page
            if current_chunk_text:
                documents.append(
                    Document(
                        page_content="\n".join(current_chunk_text),
                        metadata={
                            "page_num": page_num,
                            "source_type": blocks[-1].get("source", "digital") if blocks else "digital",
                            "section_ids": list(current_chunk_section_ids)
                        }
                    )
                )
                
        return documents
