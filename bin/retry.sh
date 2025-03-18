#!/bin/bash

# Script to automatically restart a failed command up to 3 times
# With 30 second sleep between retry attempts

# Check if command arguments are provided
if [ $# -eq 0 ]; then
    echo "Error: No command specified"
    echo "Usage: $0 command [arguments]"
    exit 1
fi

# Store the original command with all its arguments
CMD="$@"
MAX_ATTEMPTS=3
RETRY_DELAY=120

echo "Running command: $CMD"

# Initialize attempt counter
attempt=1

while [ $attempt -le $MAX_ATTEMPTS ]; do
    echo "Attempt $attempt of $MAX_ATTEMPTS"

    # Execute the command
    eval "$CMD"

    # Check exit status
    EXIT_STATUS=$?

    # If command succeeded, exit with success
    if [ $EXIT_STATUS -eq 0 ]; then
        echo "Command completed successfully on attempt $attempt"
        exit 0
    fi

    # If we've reached max attempts, exit with the last error code
    if [ $attempt -eq $MAX_ATTEMPTS ]; then
        echo "Command failed after $MAX_ATTEMPTS attempts"
        exit $EXIT_STATUS
    fi

    # Otherwise sleep and increment attempt counter
    echo "Command failed with exit code $EXIT_STATUS. Retrying in $RETRY_DELAY seconds..."
    sleep $RETRY_DELAY
    ((attempt++))
done
