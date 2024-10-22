# White Caiman

This project is a solution to an [interview question](https://github.com/TabbyML/interview-questions/tree/main/301_sync_directory_over_websocket) posed by **[TabbyML](https://tabby.tabbyml.com/)**. The task was to create a command-line tool to facilitate directory synchronization over a WebSocket connection.

## Problem Statement

The command-line tool synchronizes the contents of a directory from a **Sender** process to a **Receiver** process over a WebSocket connection. The tool ensures that both directories stay in sync, copying files from the sender and deleting files that no longer exist in the sender's directory from the receiver's directory. 

### Example Commands:

- **Receiver Process**:

  ```bash
  white-caiman listen --port 8080 --output-dir ~/Downloads/output_dir
  ```

- **Sender Process**:

  ```bash
  white-caiman sync --from ~/Downloads/input_dir --to ws://localhost:8080
  ```

Once the `sync` command completes, the receiver process will exit, and the contents of `input_dir` will be synchronized with `output_dir`.

## Features

### Minimum Requirements

- **Synchronization Semantics**:
  - If a file exists in the sender's directory but not in the receiver's directory, it will be copied.
  - If a file doesn't exist in the sender's directory but does exist in the receiver's directory, the file will be removed.
  - Directory syncing is performed recursively.

### Additional Feature: Watch Mode
- A `--watch` flag can be used to keep the connection alive and synchronize changes in real-time.
  - Inspired by Amazon's IntelliJ extension *BlackCaiman*, this feature leverages **[Watchman](https://facebook.github.io/watchman/)** to listen for file system events and transmit them in real-time to the receiver.
  - Example with watch mode enabled:

    ```bash
    white-caiman sync --from ~/Downloads/input_dir --to ws://localhost:8080 --watch
    ```

## Installation

1. **Clone the repository**:

   ```bash
   git clone https://github.com/your-repo/tabbyml-directory-sync.git
   cd tabbyml-directory-sync
   ```

2. **Build the project**:

   ```bash
   cargo build --release
   ```

3. *(Optional, needed for the 'watch' feature) Install watchman*

  Installation instructions [here](https://facebook.github.io/watchman/docs/install)

4. **Run the executable**:

   ```bash
   ./target/release/white-caiman
   ```

## Usage

The tool supports two main commands: `sync` and `listen`.

### 1. **Listen** (Receiver Process):

The `listen` command starts a receiver process that listens for incoming file synchronization events over WebSocket.

```bash
white-caiman listen --port <PORT> --out-dir-path <OUTPUT_DIR>
```

- `--port`: The port to listen on.
- `--out-dir-path`: The output directory where files will be synchronized.

### 2. **Sync** (Sender Process):

The `sync` command syncs files from the sender’s directory to the receiver over WebSocket.

```bash
white-caiman sync --from <SOURCE_DIR> --to <RECEIVER_WS_URL> [--watch]
```

- `--from`: The source directory to sync from.
- `--to`: The WebSocket URL of the receiver (e.g., `ws://localhost:8080`).
- `--watch`: (Optional) If set, the process will keep running and sync file changes in real-time.

## Running Locally

1. **Start the receiver**:

   ```bash
   white-caiman listen --port 8080 --out-dir-path ~/Downloads/output_dir
   ```

2. **Run the sender**:

   ```bash
   white-caiman sync --from ~/Downloads/input_dir --to ws://localhost:8080
   ```

With the `--watch` flag, the sender will continue watching for changes and sync them to the receiver directory.

## Additional Information

The project implements directory synchronization using two separate subcommands for sending and receiving, which provides a cleaner separation of concerns. It follows the minimum requirements, but also includes the additional `watch` feature for real-time syncing.


