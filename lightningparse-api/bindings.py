import json
from typing import Dict, Any

try:
    import lightningparse
except ImportError as e:
    raise ImportError("Failed to import lightningparse. Did you build the Rust extension with 'maturin develop'? Error: " + str(e))

def parse_pdf(path: str) -> Dict[str, Any]:
    """
    Parses a PDF file using the compiled Rust extension.
    """
    raw_result = lightningparse.parse_pdf(path)
    if isinstance(raw_result, str):
        return json.loads(raw_result)
    return raw_result
