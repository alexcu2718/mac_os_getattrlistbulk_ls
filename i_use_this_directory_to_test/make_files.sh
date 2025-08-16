#!/bin/zsh

# --- Configuration ---
# Set the total number of items (directories + files) to create.
# The user requested 200 * 1024 = 204800 items.
TOTAL_ITEMS=204800
# The number of items to show progress for (e.g., every 1000).
PROGRESS_INTERVAL=1000

# --- Script Logic ---

echo "Starting to create a total of $TOTAL_ITEMS directories and files..."
echo "This may take a while."

# Loop from 1 up to the total number of items.
for (( i=1; i <= $TOTAL_ITEMS; i++ )); do
    # Check if the loop counter is an even number.
    # We use a simple modulo check to alternate between creating files and directories.
    if (( i % 2 == 0 )); then
        # Create an empty file using the 'touch' command.
        # The naming convention is 'file_' followed by the number.
        touch "file_$i"
    else
        # Create a directory using the 'mkdir' command.
        # The naming convention is 'dir_' followed by the number.
        mkdir "dir_$i"
    fi

    # Print a progress update at regular intervals.
    if (( i % PROGRESS_INTERVAL == 0 )); then
        echo "Created $i of $TOTAL_ITEMS items."
    fi
done

echo "Script complete! Successfully created $TOTAL_ITEMS items."
