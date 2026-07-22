import lightningparse
import json

def check_pdf(name, path):
    r = json.loads(lightningparse.parse_pdf(path))
    p1 = r['pages'][0]
    print(f'[{name}] Total blocks on page 1: {len(p1["blocks"])}')
    # for i, b in enumerate(p1['blocks']):
    #    print(f'  Block {i}: bbox={b["bbox"]}')

check_pdf('draft10', 'benchmarks/corpus/draft10.pdf')
check_pdf('arxiv', 'benchmarks/corpus/arxiv_twocolumn.pdf')
