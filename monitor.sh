#!/bin/bash
# Monitor the ollama-rust/tui process and its children
APP_NAME="ollama-rust" # Or whatever the binary name ends up being, typically based on package name? 
# Package name is likely "ollama-tui" or "ollama-rust" based on cargo.toml, let's check ps.
# Assume "ollama-tui" from previous context or "ollama-rust" dir.

echo "Monitoring for '$APP_NAME' and child processes..."
echo "Press Ctrl+C to stop monitoring."

while true; do
    clear
    echo "=== Process Monitor ($(date)) ==="
    
    # Find the main PID
    # We look for the binary path usually found in target/debug/ollama-tui
    PIDS=$(pgrep -f "target/debug/ollama-tui")
    
    if [ -z "$PIDS" ]; then
        echo "Main application not running."
    else
        echo "Main Process(es):"
        ps -fp $PIDS
        
        echo ""
        echo "Child Processes (orphans if Main is Exiting):"
        # Show children of these PIDs
        for pid in $PIDS; do
            pgrep -P $pid | xargs -I {} ps -fp {} 2>/dev/null
        done
        
        # Also check for our specific reproduction sleeper if distinct
        # or any 'sleep' command just to be sure
        echo ""
        echo "Any 'sleep' commands active:"
        pgrep -a sleep
    fi
    
    sleep 1
done
