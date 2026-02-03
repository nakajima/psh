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

trap 'print_error "Failed at line $LINENO: $BASH_COMMAND"' ERR

get_cli_version() {
    grep '^version' "$SCRIPT_DIR/psh-cli/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

bump_version() {
    local current="$1"
    local bump_type="${2:-patch}"

    IFS='.' read -r major minor patch <<< "$current"

    case "$bump_type" in
        major) major=$((major + 1)); minor=0; patch=0 ;;
        minor) minor=$((minor + 1)); patch=0 ;;
        patch) patch=$((patch + 1)) ;;
    esac

    echo "$major.$minor.$patch"
}

set_cli_version() {
    local new_version="$1"
    sed -i '' "s/^version = \".*\"/version = \"$new_version\"/" "$SCRIPT_DIR/psh-cli/Cargo.toml"
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

    # Get current version from CLI
    CURRENT_VERSION=$(get_cli_version)
    if [[ -z "$CURRENT_VERSION" ]]; then
        print_error "Could not determine version from psh-cli/Cargo.toml"
        exit 1
    fi

    LAST_TAG=$(get_last_tag)

    echo ""
    echo -e "${BLUE}╔══════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║           psh Release                ║${NC}"
    echo -e "${BLUE}╚══════════════════════════════════════╝${NC}"
    echo ""

    print_info "Current version: $CURRENT_VERSION"
    if [[ -n "$LAST_TAG" ]]; then
        print_info "Previous tag: $LAST_TAG"
    else
        print_warning "No previous tags found"
    fi

    # Ask for bump type
    echo ""
    echo "Version bump type:"
    echo "  1) patch ($(bump_version "$CURRENT_VERSION" patch))"
    echo "  2) minor ($(bump_version "$CURRENT_VERSION" minor))"
    echo "  3) major ($(bump_version "$CURRENT_VERSION" major))"
    read -p "Select [1]: " bump_choice

    case "$bump_choice" in
        2) BUMP_TYPE="minor" ;;
        3) BUMP_TYPE="major" ;;
        *) BUMP_TYPE="patch" ;;
    esac

    VERSION=$(bump_version "$CURRENT_VERSION" "$BUMP_TYPE")
    TAG="v$VERSION"

    print_info "New version: $VERSION"
    print_info "Tag to create: $TAG"

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
    read -p "Bump version, create tag, and upload to TestFlight? [y/N] " confirm
    if [[ ! "$confirm" =~ ^[Yy] ]]; then
        echo "Cancelled."
        exit 0
    fi

    # Bump version in Cargo.toml
    print_info "Bumping version to $VERSION..."
    set_cli_version "$VERSION"
    git add "$SCRIPT_DIR/psh-cli/Cargo.toml"
    git commit -m "Bump version to $VERSION"
    print_success "Committed version bump"

    # Create tag
    print_info "Creating tag $TAG..."
    git tag -a "$TAG" -m "Release $VERSION"
    print_success "Created tag $TAG"

    # Push commit and tag
    print_info "Pushing to remote..."
    git push origin HEAD
    git push origin "$TAG"
    print_success "Pushed commit and tag to remote"

    # Deploy server
    print_info "Deploying server..."
    ssh root@docker "cd psh && git pull && docker compose up server --build -d"
    print_success "Server deployed"

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
