#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

VERSION="$1"
MODEL="qwen2.5-7b-instruct-1m" # Defaulting to a text-focused model as VL might be heavy/unnecessary, but user asked for qwen3-vl. 
# However, for a coding assistant context, let's use a robust default that is likely to exist or user can override.
# Actually, let's try to respect the user's request if they provided it, or use a sensible default.
# The user specifically asked for "qwen/qwen3-vl-8b".
MODEL="qwen/qwen3-vl-8b"
API_URL="http://localhost:1234/v1/chat/completions"

if [ -z "$VERSION" ]; then
    echo -e "${RED}Error: VERSION argument is missing.${NC}"
    echo "Usage: ./scripts/release.sh <x.y.z>"
    exit 1
fi

# Check for jq
if ! command -v jq &> /dev/null; then
    echo -e "${RED}Error: jq is required but not installed.${NC}"
    exit 1
fi

# Detect OS for sed compatibility
OS="$(uname)"
if [ "$OS" = "Darwin" ]; then
    SED_CMD="sed -i ''"
else
    # Linux (GNU sed)
    SED_CMD="sed -i"
fi

echo -e "${GREEN}Starting release process for version $VERSION...${NC}"

# 1. Update Cargo.toml
echo "Updating Cargo.toml..."
# Evaluate the sed command string safely
eval "$SED_CMD 's/^version = \".*\"/version = \"$VERSION\"/' Cargo.toml"

# Update Cargo.lock
echo "Updating Cargo.lock..."
cargo check > /dev/null 2>&1 || true

# 2. Stage ALL changes
echo "Staging all changes..."
git add .

# 3. Generate Commit Message
echo -e "${BLUE}Generating commit message using $MODEL...${NC}"

# Get the diff (limit to 1000 lines to avoid blowing context if huge)
DIFF_CONTENT=$(git diff --cached | head -n 2000)

# Construct JSON payload safely using jq to avoid escaping hell
# We ask for a Conventional Commit format
PROMPT="You are a senior software engineer. Write a git commit message for the following changes.
The release version is $VERSION.
Format:
<type>(<scope>): <subject>

<body>

Details:
- Summarize the key changes.
- If there are multiple changes, list them.
- Be concise but professional.

Diff:
$DIFF_CONTENT"

JSON_PAYLOAD=$(jq -n \
                  --arg model "$MODEL" \
                  --arg content "$PROMPT" \
                  '{
                    model: $model,
                    messages: [
                      {role: "system", content: "You are a helpful coding assistant."},
                      {role: "user", content: $content}
                    ],
                    temperature: 0.7,
                    stream: false
                  }')

# Call LM Studio
RESPONSE=$(curl -s -X POST "$API_URL" \
     -H "Content-Type: application/json" \
     -d "$JSON_PAYLOAD")

# Extract content
COMMIT_MSG=$(echo "$RESPONSE" | jq -r '.choices[0].message.content')

if [ -z "$COMMIT_MSG" ] || [ "$COMMIT_MSG" == "null" ]; then
    echo -e "${RED}Failed to generate commit message from LM Studio.${NC}"
    echo "Response: $RESPONSE"
    echo "Falling back to default message."
    COMMIT_MSG="chore: release version $VERSION"
else
    echo -e "${GREEN}Generated Commit Message:${NC}"
    echo "$COMMIT_MSG"
    echo "--------------------------------"
fi

# 4. Commit
echo "Committing..."
git commit -m "$COMMIT_MSG"

# 5. Tag and Push
echo "Tagging and pushing..."
git tag "v$VERSION"
git push origin master
git push origin "v$VERSION"

# 6. Update Homebrew Formula
echo "Waiting for GitHub to generate the tarball..."

TARBALL_URL="https://github.com/harryw1/intus/archive/refs/tags/v$VERSION.tar.gz"
TARBALL_FILE="v$VERSION.tar.gz"
MAX_RETRIES=10
DELAY=5

for ((i=1; i<=MAX_RETRIES; i++)); do
    echo "Attempt $i/$MAX_RETRIES: Fetching $TARBALL_URL..."
    if curl -s -L -f -o "$TARBALL_FILE" "$TARBALL_URL"; then
        echo -e "${GREEN}Tarball downloaded successfully.${NC}"
        break
    else
        echo "Tarball not ready yet. Waiting ${DELAY}s..."
        sleep $DELAY
    fi
    
    if [ $i -eq $MAX_RETRIES ]; then
        echo -e "${RED}Failed to download tarball after $MAX_RETRIES attempts.${NC}"
        exit 1
    fi
done

echo "Calculating SHA256..."
if [ "$OS" = "Darwin" ]; then
    START_SHA="$(shasum -a 256 "$TARBALL_FILE" | cut -d ' ' -f 1)"
else
    START_SHA="$(sha256sum "$TARBALL_FILE" | cut -d ' ' -f 1)"
fi

echo "New SHA256: $START_SHA"

echo "Updating Homebrew formula..."
eval "$SED_CMD \"s|url \\\".*\\\"|url \\\"$TARBALL_URL\\\"|\" homebrew/intus.rb"
eval "$SED_CMD \"s/sha256 \\\".*\\\"/sha256 \\\"$START_SHA\\\"/\" homebrew/intus.rb"

rm "$TARBALL_FILE"

# 7. Commit Formula Update
echo "Committing formula update..."
git add homebrew/intus.rb
git commit -m "fix(brew): update formula to v$VERSION"
git push origin master

echo -e "${GREEN}Release $VERSION complete!${NC}"
