import subprocess
import json

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

from hdwallet.cryptocurrencies import get_cryptocurrency


app = FastAPI()


class SymbolRequest(BaseModel):
    symbol: str


@app.post("/generate")
def generate_wallet(request: SymbolRequest):
    symbol = request.symbol

    try:
        get_cryptocurrency(symbol)
    except ValueError:
        raise HTTPException(status_code=400, detail="Invalid symbol")

    result = subprocess.run(
        ["hdwallet", "generate", "-s", symbol],
        capture_output=True,
        text=True,
        check=True
    )

    output = result.stdout.strip()
    wallet_info = json.loads(output)

    return wallet_info
