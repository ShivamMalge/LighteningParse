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
            
            for block in blocks:
                section_id = block.get("section_id", "body")
                text = block.get("text", "").strip()
                source = block.get("source", "digital")
                
                if not text:
                    continue
                    
                # If we hit a header/title, or if the chunk is too large, we break the chunk
                is_boundary = section_id in ("header", "title")
                is_too_large = (current_chunk_chars + len(text) > self.max_chars_per_chunk)
                
                if (is_boundary or is_too_large) and current_chunk_text:
                    # Flush the current chunk
                    documents.append(
                        Document(
                            page_content="\n".join(current_chunk_text),
                            metadata={
                                "page_num": page_num,
                                "source_type": source
                            }
                        )
                    )
                    current_chunk_text = []
                    current_chunk_chars = 0
                    
                # Add current block to chunk
                # We optionally include the header text in the new chunk
                current_chunk_text.append(text)
                current_chunk_chars += len(text)
                
            # Flush remaining text for the page
            if current_chunk_text:
                documents.append(
                    Document(
                        page_content="\n".join(current_chunk_text),
                        metadata={
                            "page_num": page_num,
                            # source is based on the last block, good enough for page-level homogeneity 
                            "source_type": blocks[-1].get("source", "digital") if blocks else "digital"
                        }
                    )
                )
                
        return documents
