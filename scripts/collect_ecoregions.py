#!/usr/bin/env python3
"""
Collect ecoregion data from OneEarth bioregion pages.

One-time scraper that:
1. Reads the 185 bioregion codes from our catalog
2. Fetches each bioregion page from https://www.oneearth.org/bioregions/{slug}/
3. Extracts ecoregion names and numeric IDs
4. Outputs Rust CatalogEntry literals to paste into bioregion_catalog.rs

Usage:
    python3 scripts/collect_ecoregions.py > ecoregion_entries.rs

Dependencies:
    pip install requests beautifulsoup4

URL pattern:
    https://www.oneearth.org/bioregions/{name-slug}-{code}/
    e.g. https://www.oneearth.org/bioregions/east-african-montane-forests-woodlands-at1/
"""

import re
import sys
import time
import json
from pathlib import Path

try:
    import requests
    from bs4 import BeautifulSoup
except ImportError:
    print("Install dependencies: pip install requests beautifulsoup4", file=sys.stderr)
    sys.exit(1)


# Bioregion codes and names from the catalog (code -> name)
# This is extracted from bioregion_catalog.rs
BIOREGIONS = {}


def load_bioregions_from_catalog():
    """Parse bioregion entries from the Rust catalog source."""
    catalog_path = Path(__file__).parent.parent / "crates/indras-network/src/bioregion_catalog.rs"
    if not catalog_path.exists():
        print(f"Catalog not found: {catalog_path}", file=sys.stderr)
        sys.exit(1)

    pattern = re.compile(
        r'CatalogEntry\s*\{\s*'
        r'code:\s*"([^"]+)",\s*'
        r'name:\s*"([^"]+)",\s*'
        r'level:\s*BioregionalLevel::Bioregion,'
    )

    with open(catalog_path) as f:
        content = f.read()

    for match in pattern.finditer(content):
        code = match.group(1)
        name = match.group(2)
        BIOREGIONS[code] = name


def name_to_slug(name: str, code: str) -> str:
    """Convert a bioregion name + code to a OneEarth URL slug.

    Examples:
        "East African Montane Forests & Woodlands", "AT1"
        -> "east-african-montane-forests-woodlands-at1"
    """
    # Remove special characters, lowercase, replace spaces with hyphens
    slug = name.lower()
    slug = slug.replace("&", "and")
    slug = slug.replace("'", "")
    slug = slug.replace(",", "")
    slug = slug.replace("(", "")
    slug = slug.replace(")", "")
    slug = re.sub(r'[^a-z0-9\s-]', '', slug)
    slug = re.sub(r'\s+', '-', slug.strip())
    slug = re.sub(r'-+', '-', slug)
    # Append lowercase code
    slug = f"{slug}-{code.lower()}"
    return slug


def fetch_ecoregions(code: str, name: str) -> list[dict]:
    """Fetch ecoregion data for a single bioregion page."""
    slug = name_to_slug(name, code)
    url = f"https://www.oneearth.org/bioregions/{slug}/"

    try:
        resp = requests.get(url, timeout=30, headers={
            "User-Agent": "IndrasNetwork-EcoregionCollector/1.0 (research)"
        })
        if resp.status_code == 404:
            # Try alternative slug patterns
            alt_slug = f"{slug.replace('-and-', '-')}"
            alt_url = f"https://www.oneearth.org/bioregions/{alt_slug}/"
            resp = requests.get(alt_url, timeout=30, headers={
                "User-Agent": "IndrasNetwork-EcoregionCollector/1.0 (research)"
            })

        if resp.status_code != 200:
            print(f"  WARN: {code} ({url}) returned {resp.status_code}", file=sys.stderr)
            return []

        soup = BeautifulSoup(resp.text, "html.parser")
        ecoregions = []

        # OneEarth bioregion pages list ecoregions in various formats.
        # Look for common patterns: numbered lists, tables, or specific divs.

        # Pattern 1: Look for ecoregion list items with IDs
        for item in soup.select(".ecoregion-item, .eco-list li, .bioregion-ecoregions li"):
            text = item.get_text(strip=True)
            # Try to extract numeric ID and name
            match = re.match(r'(\d+)\.\s*(.+)', text)
            if match:
                eco_id = match.group(1)
                eco_name = match.group(2).strip()
                ecoregions.append({"id": eco_id, "name": eco_name})

        # Pattern 2: Look for ecoregion links
        if not ecoregions:
            for link in soup.select('a[href*="/ecoregions/"]'):
                eco_name = link.get_text(strip=True)
                href = link.get("href", "")
                # Extract numeric ID from URL if present
                id_match = re.search(r'/ecoregions/.*?(\d+)', href)
                eco_id = id_match.group(1) if id_match else "0"
                if eco_name and len(eco_name) > 3:
                    ecoregions.append({"id": eco_id, "name": eco_name})

        # Pattern 3: Look for text content with ecoregion listings
        if not ecoregions:
            for p in soup.find_all(["p", "div", "li"]):
                text = p.get_text(strip=True)
                # Match patterns like "123. Ecoregion Name" or "Ecoregion Name (123)"
                matches = re.findall(r'(\d{2,4})\.\s+([A-Z][^.]{5,80})', text)
                for eco_id, eco_name in matches:
                    ecoregions.append({"id": eco_id, "name": eco_name.strip()})

        return ecoregions

    except requests.RequestException as e:
        print(f"  ERROR: {code} - {e}", file=sys.stderr)
        return []


def format_rust_entry(eco_id: str, eco_name: str, parent_code: str) -> str:
    """Format a single ecoregion as a Rust CatalogEntry literal."""
    # Escape any double quotes in the name
    safe_name = eco_name.replace('"', '\\"')
    return (
        f'    CatalogEntry {{ code: "{eco_id}", name: "{safe_name}", '
        f'level: BioregionalLevel::Ecoregion, parent_code: "{parent_code}" }},'
    )


def main():
    load_bioregions_from_catalog()
    print(f"Loaded {len(BIOREGIONS)} bioregions from catalog", file=sys.stderr)

    all_ecoregions = []
    failed = []

    for i, (code, name) in enumerate(sorted(BIOREGIONS.items())):
        print(f"[{i+1}/{len(BIOREGIONS)}] Fetching {code}: {name}...", file=sys.stderr)
        ecos = fetch_ecoregions(code, name)

        if ecos:
            print(f"  Found {len(ecos)} ecoregions", file=sys.stderr)
            for eco in ecos:
                all_ecoregions.append((eco["id"], eco["name"], code))
        else:
            print(f"  No ecoregions found", file=sys.stderr)
            failed.append(code)

        # Rate limiting: be respectful
        time.sleep(1.0)

    # Output Rust entries
    print(f"\n    // ── Ecoregions (auto-collected) ──")
    print(f"    // Total: {len(all_ecoregions)} ecoregions across {len(BIOREGIONS)} bioregions")
    print()
    for eco_id, eco_name, parent_code in all_ecoregions:
        print(format_rust_entry(eco_id, eco_name, parent_code))

    # Summary
    print(f"\n// Collection summary:", file=sys.stderr)
    print(f"//   Total ecoregions: {len(all_ecoregions)}", file=sys.stderr)
    print(f"//   Bioregions with ecoregions: {len(BIOREGIONS) - len(failed)}", file=sys.stderr)
    print(f"//   Bioregions without data: {len(failed)}", file=sys.stderr)
    if failed:
        print(f"//   Failed codes: {', '.join(failed)}", file=sys.stderr)

    # Also save raw JSON for debugging
    json_path = Path(__file__).parent.parent / "ecoregion_data.json"
    with open(json_path, "w") as f:
        json.dump({
            "ecoregions": [
                {"id": eid, "name": ename, "parent": parent}
                for eid, ename, parent in all_ecoregions
            ],
            "failed": failed,
        }, f, indent=2)
    print(f"\nRaw data saved to {json_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
