import asyncio
import httpx
import time
import os

async def send_request(client, api_url, pdf_path):
    with open(pdf_path, "rb") as f:
        files = {"file": ("test.pdf", f, "application/pdf")}
        response = await client.post(api_url, files=files)
        return response.status_code

async def main():
    """
    Spawns N concurrent async workers sending requests to the FastAPI /parse endpoint.
    If the GIL is correctly released in Rust, processing N parallel requests 
    should take significantly less time than N sequential requests.
    """
    api_url = "http://localhost:8000/parse"
    pdf_path = os.path.join(
        os.path.dirname(__file__), 
        "corpus/arxiv_twocolumn.pdf"
    )
    
    if not os.path.exists(pdf_path):
        print(f"Error: Could not find test PDF at {pdf_path}")
        return

    num_requests = 10
    
    print(f"Starting concurrent load test with {num_requests} requests to {api_url}...")
    
    start_time = time.time()
    
    async with httpx.AsyncClient(timeout=30.0) as client:
        tasks = [send_request(client, api_url, pdf_path) for _ in range(num_requests)]
        results = await asyncio.gather(*tasks)
        
    end_time = time.time()
    elapsed = end_time - start_time
    
    successes = sum(1 for r in results if r == 200)
    print(f"Completed {num_requests} requests in {elapsed:.2f} seconds.")
    print(f"Successful requests (200 OK): {successes} / {num_requests}")
    
    if elapsed < 2.0:
        print("GIL release test likely PASSED: Concurrent processing was fast.")
    else:
        print("GIL release test might have FAILED or the system is under load.")

if __name__ == "__main__":
    asyncio.run(main())
