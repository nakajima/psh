#!/bin/bash

# psh Release Script
# Creates a git tag and uploads to TestFlight with changelog

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

print_success() { echo -e "${GREEN}✓ $1${NC}"; }
print_warning() { echo -e "${YELLOW}! $1${NC}"; }
print_error() { echo -e "${RED}✗ $1${NC}"; }
print_info() { echo -e "${BLUE}→ $1${NC}"; }

get_crate_version() {
    local cargo_toml="$1"
    grep '^version' "$cargo_toml" | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

check_versions_in_sync() {
    local cli_version=$(get_crate_version "$SCRIPT_DIR/psh-cli/Cargo.toml")
    local server_version=$(get_crate_version "$SCRIPT_DIR/server/Cargo.toml")

    if [[ "$cli_version" != "$server_version" ]]; then
        print_error "Version mismatch: CLI=$cli_version, server=$server_version"
        print_info "Update both Cargo.toml files to the same version before releasing"
        exit 1
    fi

    echo "$cli_version"
}

get_last_tag() {
    git describe --tags --abbrev=0 2>/dev/null || echo ""
}

get_changelog() {
    local last_tag="$1"
    if [[ -n "$last_tag" ]]; then
        git log "$last_tag"..HEAD --pretty=format:"• %s" --no-merges
    else
        git log --pretty=format:"• %s" --no-merges -20
    fi
}

main() {
    cd "$SCRIPT_DIR"

    # Check for uncommitted changes
    if [[ -n $(git status --porcelain) ]]; then
        print_error "Working directory has uncommitted changes. Please commit or stash them first."
        exit 1
    fi

    # Get current version (ensures CLI and server are in sync)
    VERSION=$(check_versions_in_sync)
    if [[ -z "$VERSION" ]]; then
        print_error "Could not determine version"
        exit 1
    fi

    TAG="v$VERSION"
    LAST_TAG=$(get_last_tag)

    echo ""
    echo -e "${BLUE}╔══════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║           psh Release                ║${NC}"
    echo -e "${BLUE}╚══════════════════════════════════════╝${NC}"
    echo ""

    print_info "Current version: $VERSION"
    print_info "Tag to create: $TAG"
    if [[ -n "$LAST_TAG" ]]; then
        print_info "Previous tag: $LAST_TAG"
    else
        print_warning "No previous tags found"
    fi

    # Check if tag already exists
    if git rev-parse "$TAG" >/dev/null 2>&1; then
        print_error "Tag $TAG already exists"
        exit 1
    fi

    # Generate changelog
    echo ""
    echo -e "${BLUE}Changelog:${NC}"
    CHANGELOG=$(get_changelog "$LAST_TAG")
    if [[ -z "$CHANGELOG" ]]; then
        CHANGELOG="• Initial release"
    fi
    echo "$CHANGELOG"
    echo ""

    # Confirm
    read -p "Create tag and upload to TestFlight? [y/N] " confirm
    if [[ ! "$confirm" =~ ^[Yy] ]]; then
        echo "Cancelled."
        exit 0
    fi

    # Create tag
    print_info "Creating tag $TAG..."
    git tag -a "$TAG" -m "Release $VERSION"
    print_success "Created tag $TAG"

    # Push tag
    print_info "Pushing tag to remote..."
    git push origin "$TAG"
    print_success "Pushed tag to remote"

    # Run fastlane beta with changelog
    print_info "Building and uploading to TestFlight..."
    cd "$SCRIPT_DIR"

    # Export changelog for fastlane
    export FASTLANE_CHANGELOG="$CHANGELOG"

    bundle exec fastlane beta changelog:"$CHANGELOG"

    echo ""
    print_success "Release $VERSION complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Wait for TestFlight processing"
    echo "  2. Add release notes in App Store Connect if needed"
}

main "$@"
