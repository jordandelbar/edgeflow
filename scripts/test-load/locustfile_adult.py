"""
Adult income inference load test — varied payloads.

Unlike the generic locustfile.py (which replays one static payload), this
script generates a random realistic adult-income profile for every request.
This exercises the encoding path more faithfully and avoids any caching
effects from a repeated body.

Environment variables
---------------------
EDGEFLOW_SERVER   Base URL of the edgeflow server  (default: http://localhost:5000)
EDGEFLOW_TARGET   Target name to benchmark          (default: adult-inference)

Quick-start
-----------
EDGEFLOW_TARGET=adult-inference \
locust -f locustfile_adult.py --headless -u 10 -r 2 -t 60s

If locust is CPU-bound (server answers faster than locust can generate load),
add --processes to spawn one worker per core:
EDGEFLOW_TARGET=adult-inference \
locust -f locustfile_adult.py --headless -u 100 -r 10 -t 60s --processes 4
"""

import json
import os
import random

import requests
from locust import between, events, task
from locust.contrib.fasthttp import FastHttpUser

# ── configuration ─────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
TARGET = os.environ.get("EDGEFLOW_TARGET", "adult-inference")
# Override the resolved pod address (e.g. when port-forwarding from outside the cluster).
INFER_HOST = os.environ.get("INFER_HOST", "")
ENDPOINT = "/infer"

# ── realistic feature distributions (from UCI Adult Income dataset) ────────────

_WORKCLASSES = [
    "Private",
    "Private",
    "Private",
    "Private",  # ~70 % private
    "Self-emp-not-inc",
    "Self-emp-inc",
    "Local-gov",
    "State-gov",
    "Federal-gov",
    "Without-pay",
]

_EDUCATIONS = [
    "HS-grad",
    "HS-grad",
    "HS-grad",  # most common
    "Some-college",
    "Some-college",
    "Bachelors",
    "Bachelors",
    "Masters",
    "Assoc-voc",
    "Assoc-acdm",
    "11th",
    "10th",
    "9th",
    "Prof-school",
    "Doctorate",
]

_MARITAL_STATUSES = [
    "Married-civ-spouse",
    "Married-civ-spouse",  # most common
    "Never-married",
    "Never-married",
    "Divorced",
    "Separated",
    "Widowed",
    "Married-spouse-absent",
]

_OCCUPATIONS = [
    "Prof-specialty",
    "Craft-repair",
    "Exec-managerial",
    "Adm-clerical",
    "Sales",
    "Other-service",
    "Machine-op-inspct",
    "Transport-moving",
    "Handlers-cleaners",
    "Farming-fishing",
    "Tech-support",
    "Protective-serv",
    "Priv-house-serv",
    "Armed-Forces",
]

_RELATIONSHIPS = [
    "Husband",
    "Husband",  # most common
    "Not-in-family",
    "Not-in-family",
    "Own-child",
    "Unmarried",
    "Wife",
    "Other-relative",
]

_RACES = [
    "White",
    "White",
    "White",
    "White",  # ~85 %
    "Black",
    "Asian-Pac-Islander",
    "Amer-Indian-Eskimo",
    "Other",
]

_SEXES = ["Male", "Male", "Female"]  # ~67 % male in dataset

_COUNTRIES = [
    "United-States",
    "United-States",
    "United-States",  # ~90 %
    "Mexico",
    "Philippines",
    "Germany",
    "Canada",
    "India",
    "El-Salvador",
    "Cuba",
    "South",
]

_EDUCATION_NUM = {
    "Preschool": 1,
    "1st-4th": 2,
    "5th-6th": 3,
    "7th-8th": 4,
    "9th": 5,
    "10th": 6,
    "11th": 7,
    "12th": 8,
    "HS-grad": 9,
    "Some-college": 10,
    "Assoc-voc": 11,
    "Assoc-acdm": 12,
    "Bachelors": 13,
    "Masters": 14,
    "Prof-school": 15,
    "Doctorate": 16,
}


def _random_profile() -> bytes:
    education = random.choice(_EDUCATIONS)
    return json.dumps(
        {
            "workclass": random.choice(_WORKCLASSES),
            "education": education,
            "marital-status": random.choice(_MARITAL_STATUSES),
            "occupation": random.choice(_OCCUPATIONS),
            "relationship": random.choice(_RELATIONSHIPS),
            "race": random.choice(_RACES),
            "sex": random.choice(_SEXES),
            "native-country": random.choice(_COUNTRIES),
            "age": random.randint(18, 75),
            "fnlwgt": random.randint(12_000, 1_500_000),
            "education-num": _EDUCATION_NUM.get(education, 9),
            "capital-gain": random.choices(
                [0, random.randint(1, 99_999)], weights=[85, 15]
            )[0],
            "capital-loss": random.choices(
                [0, random.randint(1, 4_356)], weights=[95, 5]
            )[0],
            "hours-per-week": random.randint(1, 99),
        }
    ).encode()


# ── pod address resolution ────────────────────────────────────────────────────


def _resolve_pod_address(server: str, target: str) -> str:
    resp = requests.get(f"{server}/api/v1/targets", timeout=5)
    resp.raise_for_status()
    targets = resp.json().get("targets", [])
    for t in targets:
        if t["target"] == target:
            pods = t.get("pods", [])
            if not pods:
                raise SystemExit(f"[locust] target '{target}' has no registered pods")
            addr = pods[0]["address"]
            if not addr.startswith("http"):
                addr = f"http://{addr}"
            print(f"[locust] target '{target}' → {addr}")
            return addr
    raise SystemExit(
        f"[locust] target '{target}' not registered on {server}.\n"
        "  Available targets: " + ", ".join(t["target"] for t in targets)
    )


_pod_address = (
    INFER_HOST if INFER_HOST else _resolve_pod_address(EDGEFLOW_SERVER, TARGET)
)

# ── user ──────────────────────────────────────────────────────────────────────


class AdultIncomeUser(FastHttpUser):
    host = _pod_address
    wait_time = between(0, 0)

    @task
    def infer(self):
        with self.client.post(
            ENDPOINT,
            data=_random_profile(),
            headers={"content-type": "application/json"},
            catch_response=True,
        ) as resp:
            if resp.status_code == 200:
                resp.success()
            elif resp.status_code == 429:
                resp.success()
            elif resp.status_code == 503:
                resp.failure("503 no model loaded")
            else:
                resp.failure(
                    f"unexpected {resp.status_code}: {(resp.text or '')[:120]}"
                )


# ── startup banner ────────────────────────────────────────────────────────────


@events.test_start.add_listener
def on_test_start(environment, **_kwargs):
    print(
        f"\n{'─' * 60}\n"
        f"  target      : {TARGET}\n"
        f"  pod address : {_pod_address}\n"
        f"  payload     : random adult-income profile per request\n"
        f"  content-type: application/json\n"
        f"{'─' * 60}\n"
    )
