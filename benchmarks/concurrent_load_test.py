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
    Spawns requests to the FastAPI /parse endpoint to measure GIL release.
    Compares single-request baseline, sequential 10-request baseline, and concurrent 10-request test.
    """
    api_url = "http://localhost:8000/parse"
    pdf_path = os.path.join(
        os.path.dirname(__file__), 
        "../lightningparse-core/tests/fixtures/tier2/mixed_test.pdf"
    )
    
    if not os.path.exists(pdf_path):
        print(f"Error: Could not find test PDF at {pdf_path}")
        return

    num_requests = 10
    
    async with httpx.AsyncClient(timeout=120.0) as client:
        # 1. Untimed Warm-up Request
        print(f"--- 1. Untimed Warm-up Request ---")
        await send_request(client, api_url, pdf_path)
        print("Warm-up complete. File cache is hot.\n")

        # 2. Sequential 10-request Baseline
        print(f"--- 2. Sequential {num_requests}-request Baseline ---")
        start_seq = time.time()
        for _ in range(num_requests):
            await send_request(client, api_url, pdf_path)
        seq_elapsed = time.time() - start_seq
        print(f"Sequential {num_requests} requests completed in {seq_elapsed:.2f} seconds.\n")

        # 3. Concurrent 10-request Test
        print(f"--- 3. Concurrent 10-request Test ---")
        start_conc = time.time()
        tasks = [send_request(client, api_url, pdf_path) for _ in range(num_requests)]
        results = await asyncio.gather(*tasks)
        conc_elapsed = time.time() - start_conc
        
        successes = sum(1 for r in results if r == 200)
        print(f"Concurrent {num_requests} requests completed in {conc_elapsed:.2f} seconds.")
        print(f"Successful requests (200 OK): {successes} / {num_requests}\n")
        
    print(f"--- Summary ---")
    print(f"Sequential {num_requests} requests time: {seq_elapsed:.2f}s")
    print(f"Concurrent {num_requests} requests time: {conc_elapsed:.2f}s")
    print(f"Speedup vs Sequential: {seq_elapsed / conc_elapsed:.2f}x")
    
    if conc_elapsed < seq_elapsed * 0.95:
        print("\nGIL release test PASSED: Concurrent processing was faster than sequential, proving GIL release.")
    else:
        print("\nGIL release test might have FAILED: Concurrent processing was not faster (GIL might be held, or CPU is fully saturated).")

if __name__ == "__main__":
    asyncio.run(main())
