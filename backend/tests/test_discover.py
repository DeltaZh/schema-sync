from app.discover import match_databases


def test_order_year_tenant_pattern():
    names = ["order_2025_lemi", "order_2026_whd", "product_lemi", "mysql"]
    assert match_databases(names, "order_*_*") == ["order_2025_lemi", "order_2026_whd"]


def test_exclude_template():
    names = ["order_2025_lemi", "order_2026_whd"]
    assert match_databases(names, "order_*_*", exclude="order_2025_lemi") == ["order_2026_whd"]


def test_product_pattern():
    names = ["product_lemi", "product_whd", "order_2025_lemi"]
    assert match_databases(names, "product_*") == ["product_lemi", "product_whd"]
