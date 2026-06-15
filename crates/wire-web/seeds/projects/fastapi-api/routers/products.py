from fastapi import APIRouter

router = APIRouter(prefix="/api")


@router.get("/products")
def list_products():
    return []


@router.get("/products/{product_id}")
def get_product(product_id: int):
    return {"id": product_id}


@router.post("/products")
def create_product(product: dict):
    return product
