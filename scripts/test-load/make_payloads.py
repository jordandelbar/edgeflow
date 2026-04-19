"""
Generate sample request payloads for the Locust load test.

Usage:
    python make_payloads.py

Outputs (in ./payloads/):
    iris.bin     - 16 bytes: 4 × f32 LE (typical iris sample)
    adult.json   - JSON body for the adult-income named-input model

For yolov8 you need a real JPEG - copy any test image:
    cp /path/to/image.jpg payloads/sample.jpg
"""

import json
import os
import struct

os.makedirs("payloads", exist_ok=True)

# ── iris: sepal_len=5.1, sepal_w=3.5, petal_len=1.4, petal_w=0.2 ─────────────
iris_bytes = struct.pack("<4f", 5.1, 3.5, 1.4, 0.2)
with open("payloads/iris.bin", "wb") as f:
    f.write(iris_bytes)
print(f"wrote payloads/iris.bin ({len(iris_bytes)} bytes)")

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

print("\nFor yolov8, supply a JPEG manually:")
print("  cp /path/to/image.jpg payloads/sample.jpg")
