.PHONY: check-schemas

# Validate RPC and event schemas and their examples (docs/architecture/schemas.md).
# Needs jq and jsonschema-cli (cargo install jsonschema-cli).
check-schemas:
	@scripts/check-schemas.sh
