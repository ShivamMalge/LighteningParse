import pytest
import os
import tempfile
from fastapi.testclient import TestClient
from api.main import app
from unittest.mock import patch, MagicMock

client = TestClient(app)

@pytest.fixture
def sample_pdf_path():
    # We will use the digital tier fixture from core
    # Since lightningparse-api is next to lightningparse-core
    path = os.path.join(
        os.path.dirname(__file__), 
        "../../benchmarks/corpus/arxiv_twocolumn.pdf"
    )
    if not os.path.exists(path):
        # Fallback if path is wrong during test
        pytest.skip("Test fixture PDF not found")
    return path

@patch("api.main.DocumentRetriever")
@patch("api.main.QAChain")
@patch("api.main.OpenAIProvider")
def test_ask_endpoint_mocked(mock_provider, mock_qa_chain_cls, mock_retriever_cls, sample_pdf_path):
    """
    Tests the /ask endpoint end-to-end but mocks the LLM/Embeddings 
    to avoid requiring an OpenAI API key or incurring costs.
    """
    # Setup mocks
    mock_qa_instance = MagicMock()
    # When QAChain is invoked, return a mock response
    mock_qa_instance.invoke.return_value = {
        "answer": "This is a mocked answer about the PDF.",
        "context": [
            MagicMock(page_content="Mocked chunk text", metadata={"page_num": 1, "source_type": "digital"})
        ]
    }
    mock_qa_chain_cls.return_value = mock_qa_instance

    with open(sample_pdf_path, "rb") as f:
        response = client.post(
            "/ask",
            files={"file": ("test.pdf", f, "application/pdf")},
            data={"query": "What is this document about?"}
        )

    assert response.status_code == 200
    data = response.json()
    
    assert "answer" in data
    assert data["answer"] == "This is a mocked answer about the PDF."
    
    assert "citations" in data
    assert len(data["citations"]) == 1
    assert data["citations"][0]["page_num"] == 1

def test_parse_endpoint(sample_pdf_path):
    """
    Tests the /parse endpoint to ensure Rust FFI integration works.
    """
    with open(sample_pdf_path, "rb") as f:
        response = client.post(
            "/parse",
            files={"file": ("test.pdf", f, "application/pdf")}
        )

    assert response.status_code == 200
    data = response.json()
    assert "pages" in data
    assert len(data["pages"]) > 0
    assert data["pages"][0]["page_num"] == 1
