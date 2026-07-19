from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Dict, Any

app = FastAPI(title="LightningParse API")

class HealthResponse(BaseModel):
    status: str

class ParseResponse(BaseModel):
    status: str
    message: str

@app.get("/health", response_model=HealthResponse)
def health_check() -> HealthResponse:
    return HealthResponse(status="ok")

@app.post("/parse", response_model=ParseResponse)
def parse_document() -> ParseResponse:
    raise HTTPException(status_code=501, detail="Not Implemented")
