#!/bin/bash

# psh Setup Wizard
# Configures project-specific values for your deployment

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MARKER_FILE="$SCRIPT_DIR/.setup-complete"

# Default values (to be replaced)
DEFAULT_SERVER_URL="https://psh.fishmt.net"
DEFAULT_BUNDLE_ID="fm.folder.psh"
DEFAULT_TEAM_ID="Z773AM52SJ"

# Flags
FORCE=false

# Values to set
SERVER_URL=""
BUNDLE_ID=""
TEAM_ID=""

print_header() {
    echo -e "${BLUE}"
    echo "╔══════════════════════════════════════╗"
    echo "║          psh Setup Wizard            ║"
    echo "╚══════════════════════════════════════╝"
    echo -e "${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}! $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${BLUE}→ $1${NC}"
}

usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Options:
  --force              Re-run setup even if already completed
  -h, --help           Show this help message

Examples:
  $0                   # Run setup wizard
  $0 --force           # Re-run setup wizard
EOF
    exit 0
}

validate_url() {
    local url="$1"
    if [[ ! "$url" =~ ^https?:// ]]; then
        return 1
    fi
    return 0
}

validate_bundle_id() {
    local bundle_id="$1"
    # Bundle ID should be reverse domain notation: com.example.app
    if [[ ! "$bundle_id" =~ ^[a-zA-Z][a-zA-Z0-9-]*(\.[a-zA-Z][a-zA-Z0-9-]*)+$ ]]; then
        return 1
    fi
    return 0
}

validate_team_id() {
    local team_id="$1"
    # Team ID should be exactly 10 alphanumeric characters
    if [[ ! "$team_id" =~ ^[A-Z0-9]{10}$ ]]; then
        return 1
    fi
    return 0
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --force)
                FORCE=true
                shift
                ;;
            -h|--help)
                usage
                ;;
            *)
                print_error "Unknown option: $1"
                usage
                ;;
        esac
    done
}

check_already_run() {
    if [[ -f "$MARKER_FILE" ]] && [[ "$FORCE" != true ]]; then
        print_warning "Setup has already been completed."
        echo "Run with --force to reconfigure."
        exit 0
    fi
}

prompt_server_url() {
    echo ""
    echo -e "${BLUE}Server URL${NC}"
    echo "Enter the URL of your psh server."
    echo ""
    while true; do
        read -p "Server URL: " SERVER_URL
        if [[ -z "$SERVER_URL" ]]; then
            print_error "Server URL is required"
        elif ! validate_url "$SERVER_URL"; then
            print_error "Invalid URL format. Must start with http:// or https://"
        else
            break
        fi
    done
}

prompt_bundle_id() {
    echo ""
    echo -e "${BLUE}Bundle Identifier${NC} (optional)"
    echo "The app bundle ID (e.g., com.yourcompany.psh)"
    echo "Press Enter to keep the current value."
    echo ""
    read -p "Bundle ID [$DEFAULT_BUNDLE_ID]: " input
    if [[ -n "$input" ]]; then
        if ! validate_bundle_id "$input"; then
            print_error "Invalid bundle ID format. Using default."
        else
            BUNDLE_ID="$input"
        fi
    fi
}

prompt_team_id() {
    echo ""
    echo -e "${BLUE}Apple Team ID${NC} (optional)"
    echo "Your 10-character Apple Developer Team ID"
    echo "Press Enter to keep the current value."
    echo ""
    read -p "Team ID [$DEFAULT_TEAM_ID]: " input
    if [[ -n "$input" ]]; then
        if ! validate_team_id "$input"; then
            print_error "Invalid Team ID format. Must be 10 alphanumeric characters. Using default."
        else
            TEAM_ID="$input"
        fi
    fi
}

update_files() {
    echo ""
    print_info "Updating configuration files..."

    # Update server URL in APIClient.swift
    if [[ -n "$SERVER_URL" ]]; then
        local api_client="$SCRIPT_DIR/psh/APIClient.swift"
        if [[ -f "$api_client" ]]; then
            sed -i '' "s|$DEFAULT_SERVER_URL|$SERVER_URL|g" "$api_client"
            print_success "Updated server URL in APIClient.swift"
        fi
    fi

    # Update bundle ID in multiple files
    if [[ -n "$BUNDLE_ID" ]]; then
        local files=(
            "$SCRIPT_DIR/psh.xcodeproj/project.pbxproj"
            "$SCRIPT_DIR/fastlane/Appfile"
            "$SCRIPT_DIR/docker-compose.yml"
            "$SCRIPT_DIR/.env.example"
        )
        for file in "${files[@]}"; do
            if [[ -f "$file" ]]; then
                sed -i '' "s|$DEFAULT_BUNDLE_ID|$BUNDLE_ID|g" "$file"
                print_success "Updated bundle ID in $(basename "$file")"
            fi
        done
    fi

    # Update team ID in Xcode project and fastlane
    if [[ -n "$TEAM_ID" ]]; then
        local files=(
            "$SCRIPT_DIR/psh.xcodeproj/project.pbxproj"
            "$SCRIPT_DIR/fastlane/Appfile"
        )
        for file in "${files[@]}"; do
            if [[ -f "$file" ]]; then
                sed -i '' "s|$DEFAULT_TEAM_ID|$TEAM_ID|g" "$file"
                print_success "Updated team ID in $(basename "$file")"
            fi
        done
    fi

    # Create marker file
    echo "Setup completed on $(date)" > "$MARKER_FILE"
    echo "Server URL: ${SERVER_URL:-$DEFAULT_SERVER_URL}" >> "$MARKER_FILE"
    echo "Bundle ID: ${BUNDLE_ID:-$DEFAULT_BUNDLE_ID}" >> "$MARKER_FILE"
    echo "Team ID: ${TEAM_ID:-$DEFAULT_TEAM_ID}" >> "$MARKER_FILE"
}

main() {
    parse_args "$@"

    print_header
    check_already_run

    echo "This wizard will configure psh for your environment."
    echo ""

    prompt_server_url
    prompt_bundle_id
    prompt_team_id

    echo ""
    echo -e "${BLUE}Summary:${NC}"
    echo "  Server URL: $SERVER_URL"
    echo "  Bundle ID:  ${BUNDLE_ID:-$DEFAULT_BUNDLE_ID (unchanged)}"
    echo "  Team ID:    ${TEAM_ID:-$DEFAULT_TEAM_ID (unchanged)}"
    echo ""

    read -p "Apply these changes? [Y/n] " confirm
    if [[ "$confirm" =~ ^[Nn] ]]; then
        echo "Setup cancelled."
        exit 0
    fi

    update_files

    echo ""
    print_success "Setup complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Open psh.xcodeproj in Xcode"
    echo "  2. Update signing settings if needed"
    echo "  3. Build and run the app"
}

main "$@"
