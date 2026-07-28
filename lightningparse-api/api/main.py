from fastapi import FastAPI, HTTPException, UploadFile, File, Form
from pydantic import BaseModel
from typing import Dict, Any, List
import os
import json
import tempfile
import lightningparse

from chunking.chunker import MetadataAwareChunker
from pipeline.openai_provider import OpenAIProvider
from pipeline.retriever import DocumentRetriever
from pipeline.qa_chain import QAChain

app = FastAPI(title="LightningParse API")

class HealthResponse(BaseModel):
    status: str

@app.get("/health", response_model=HealthResponse)
def health_check() -> HealthResponse:
    return HealthResponse(status="ok")

@app.post("/parse")
async def parse_document(file: UploadFile = File(...)):
    """
    Parses a PDF file and returns the structured JSON output from the Rust core.
    """
    if not file.filename.lower().endswith(".pdf"):
        raise HTTPException(status_code=400, detail="Only PDF files are supported.")
        
    with tempfile.NamedTemporaryFile(delete=False, suffix=".pdf") as tmp:
        tmp.write(await file.read())
        tmp_path = tmp.name
        
    try:
        # Calls the Rust core (GIL is released internally)
        json_str = lightningparse.parse_pdf(tmp_path)
        return json.loads(json_str)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
    finally:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)

@app.post("/ask")
async def ask_document(
    file: UploadFile = File(...),
    query: str = Form(...)
):
    """
    End-to-end RAG endpoint. Parses the PDF, chunks it, embeds it, and answers the query.
    """
    if not file.filename.lower().endswith(".pdf"):
        raise HTTPException(status_code=400, detail="Only PDF files are supported.")
        
    # 1. Parse PDF
    with tempfile.NamedTemporaryFile(delete=False, suffix=".pdf") as tmp:
        tmp.write(await file.read())
        tmp_path = tmp.name
        
    try:
        json_str = lightningparse.parse_pdf(tmp_path)
    except Exception as e:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
        raise HTTPException(status_code=500, detail=f"Failed to parse PDF: {str(e)}")
    finally:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
            
    # 2. Chunking
    chunker = MetadataAwareChunker(max_chars_per_chunk=1500)
    documents = chunker.chunk(json_str)
    
    if not documents:
        raise HTTPException(status_code=400, detail="No extractable text found in the PDF.")
        
    # 3. Embedding and Retrieval
    try:
        provider = OpenAIProvider()
    except ValueError as e:
        raise HTTPException(status_code=500, detail=str(e))
        
    doc_retriever = DocumentRetriever(provider)
    retriever = doc_retriever.build_retriever(documents)
    
    # 4. QA Chain
    qa_chain = QAChain(provider, retriever)
    result = qa_chain.invoke(query)
    
    # Format the context for the response
    citations = []
    for doc in result["context"]:
        citations.append({
            "text": doc.page_content,
            "page_num": doc.metadata.get("page_num"),
            "source_type": doc.metadata.get("source_type")
        })
        
    return {
        "answer": result["answer"],
        "citations": citations
    }

