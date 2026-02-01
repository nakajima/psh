#!/bin/bash

# psh Setup Wizard
# Creates Config.xcconfig with your project-specific values

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE="$SCRIPT_DIR/Config.xcconfig"

print_success() { echo -e "${GREEN}✓ $1${NC}"; }
print_warning() { echo -e "${YELLOW}! $1${NC}"; }
print_error() { echo -e "${RED}✗ $1${NC}"; }
print_info() { echo -e "${BLUE}→ $1${NC}"; }

validate_bundle_id() {
    local bundle_id="$1"
    if [[ ! "$bundle_id" =~ ^[a-zA-Z][a-zA-Z0-9-]*(\.[a-zA-Z][a-zA-Z0-9-]*)+$ ]]; then
        return 1
    fi
    return 0
}

validate_team_id() {
    local team_id="$1"
    if [[ ! "$team_id" =~ ^[A-Z0-9]{10}$ ]]; then
        return 1
    fi
    return 0
}

print_header() {
    echo -e "${BLUE}"
    echo "╔══════════════════════════════════════╗"
    echo "║          psh Setup Wizard            ║"
    echo "╚══════════════════════════════════════╝"
    echo -e "${NC}"
}

main() {
    print_header

    if [[ -f "$CONFIG_FILE" ]]; then
        print_warning "Config.xcconfig already exists."
        read -p "Overwrite? [y/N] " confirm
        if [[ ! "$confirm" =~ ^[Yy] ]]; then
            echo "Setup cancelled."
            exit 0
        fi
    fi

    echo "This wizard will create Config.xcconfig for your environment."
    echo ""

    # Prompt for Team ID
    echo -e "${BLUE}Apple Team ID${NC}"
    echo "Your 10-character Apple Developer Team ID"
    echo ""
    while true; do
        read -p "Team ID: " TEAM_ID
        if [[ -z "$TEAM_ID" ]]; then
            print_error "Team ID is required"
        elif ! validate_team_id "$TEAM_ID"; then
            print_error "Invalid Team ID format. Must be 10 alphanumeric characters."
        else
            break
        fi
    done

    # Prompt for Bundle ID
    echo ""
    echo -e "${BLUE}Bundle Identifier${NC}"
    echo "The app bundle ID (e.g., com.yourcompany.psh)"
    echo ""
    while true; do
        read -p "Bundle ID: " BUNDLE_ID
        if [[ -z "$BUNDLE_ID" ]]; then
            print_error "Bundle ID is required"
        elif ! validate_bundle_id "$BUNDLE_ID"; then
            print_error "Invalid bundle ID format"
        else
            break
        fi
    done

    # Summary
    echo ""
    echo -e "${BLUE}Summary:${NC}"
    echo "  Team ID:    $TEAM_ID"
    echo "  Bundle ID:  $BUNDLE_ID"
    echo ""

    read -p "Create Config.xcconfig? [Y/n] " confirm
    if [[ "$confirm" =~ ^[Nn] ]]; then
        echo "Setup cancelled."
        exit 0
    fi

    # Write config file
    cat > "$CONFIG_FILE" << EOF
// psh Configuration
// This file is gitignored - copy from Config.xcconfig.example

DEVELOPMENT_TEAM = $TEAM_ID
PSH_BUNDLE_IDENTIFIER = $BUNDLE_ID
EOF

    print_success "Created Config.xcconfig"
    echo ""
    echo "Next steps:"
    echo "  1. Open psh.xcodeproj in Xcode"
    echo "  2. Build and run the app"
}

main "$@"
