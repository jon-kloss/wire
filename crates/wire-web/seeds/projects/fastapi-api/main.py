from fastapi import FastAPI

from routers import products

app = FastAPI(title="Sample FastAPI")
app.include_router(products.router)


@app.get("/health")
def health():
    return {"status": "ok"}
