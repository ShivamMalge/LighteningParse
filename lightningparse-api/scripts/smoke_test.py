import os
import requests
import json
import sys

def run_smoke_test():
    """
    Opt-in end-to-end smoke test using the actual FastAPI /ask endpoint.
    Requires OPENAI_API_KEY to be set in the environment and the server to be running.
    """
    if not os.getenv("OPENAI_API_KEY"):
        print("Error: OPENAI_API_KEY environment variable is required.")
        sys.exit(1)
        
    api_url = "http://localhost:8000/ask"
    
    # Use the benchmark fixture
    pdf_path = os.path.join(
        os.path.dirname(__file__), 
        "../../benchmarks/corpus/arxiv_twocolumn.pdf"
    )
    
    if not os.path.exists(pdf_path):
        print(f"Error: Could not find test PDF at {pdf_path}")
        sys.exit(1)
        
    query = "What is the main topic of this paper?"
    
    print(f"Sending real request to {api_url}...")
    with open(pdf_path, "rb") as f:
        files = {"file": ("arxiv_twocolumn.pdf", f, "application/pdf")}
        data = {"query": query}
        
        response = requests.post(api_url, files=files, data=data)
        
    if response.status_code != 200:
        print(f"Failed with status code {response.status_code}")
        print(response.text)
        sys.exit(1)
        
    result = response.json()
    print("Success! Got response:")
    print("="*40)
    print("Answer:", result.get("answer"))
    print("="*40)
    print(f"Citations: {len(result.get('citations', []))} chunks retrieved.")
    for i, citation in enumerate(result.get("citations", [])):
        print(f"[{i+1}] Page {citation.get('page_num')} - Source: {citation.get('source_type')}")

if __name__ == "__main__":
    run_smoke_test()
