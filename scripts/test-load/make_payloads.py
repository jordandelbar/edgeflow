"""
Generate sample request payloads for the Locust load test.

Usage:
    python make_payloads.py

Outputs (in ./payloads/):
    iris.json    - JSON array body: [5.1, 3.5, 1.4, 0.2]
    adult.json   - JSON body for the adult-income named-input model
    sample.jpg   - canonical ultralytics YOLO test image (bus.jpg)
"""

import json
import os
import urllib.request

os.makedirs("payloads", exist_ok=True)

# ── iris: sepal_len=5.1, sepal_w=3.5, petal_len=1.4, petal_w=0.2 ─────────────
iris_bytes = json.dumps([5.1, 3.5, 1.4, 0.2]).encode()
with open("payloads/iris.json", "wb") as f:
    f.write(iris_bytes)
print(f"wrote payloads/iris.json ({len(iris_bytes)} bytes)")

# ── adult income: representative "high-earner" profile ───────────────────────
adult = {
    "workclass": "Private",
    "education": "Bachelors",
    "marital-status": "Married-civ-spouse",
    "occupation": "Exec-managerial",
    "relationship": "Husband",
    "race": "White",
    "sex": "Male",
    "native-country": "United-States",
    "age": 45,
    "fnlwgt": 200000,
    "education-num": 13,
    "capital-gain": 0,
    "capital-loss": 0,
    "hours-per-week": 40,
}
adult_bytes = json.dumps(adult).encode()
with open("payloads/adult.json", "wb") as f:
    f.write(adult_bytes)
print(f"wrote payloads/adult.json ({len(adult_bytes)} bytes)")

# ── yolov8: download ultralytics' canonical test image (bus.jpg) ─────────────
# Skip the download if the file already exists - lets users drop in their own
# JPEG without having `make_payloads.py` overwrite it.
sample_path = "payloads/sample.jpg"
if os.path.exists(sample_path):
    print(f"using existing {sample_path} ({os.path.getsize(sample_path)} bytes)")
else:
    url = "https://ultralytics.com/images/bus.jpg"
    print(f"downloading {url} -> {sample_path} ...")
    urllib.request.urlretrieve(url, sample_path)
    print(f"wrote {sample_path} ({os.path.getsize(sample_path)} bytes)")
