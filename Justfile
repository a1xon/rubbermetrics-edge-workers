# Justfile for RubberMetrics Edge Workers

# Run all tests
test:
    cargo test --workspace

# Dev servers
dev-search:
    cd search && npx wrangler dev

dev-elo:
    cd international-elo && npx wrangler dev

dev-flight:
    cd flightcurve && npx wrangler dev

# Deploy everything
deploy-all:
    cd search && npx wrangler deploy
    cd flightcurve && npx wrangler deploy
    cd international-elo && npx wrangler deploy
