#!/usr/bin/env python3
import os

# Get list of files in current directory
files = os.listdir('.')

# Filter out directories if needed (optional)
# files = [f for f in files if os.path.isfile(f)]

# Count files
file_count = len(files)

print(f"Files in current directory: {file_count}")
print("Files:", files)
