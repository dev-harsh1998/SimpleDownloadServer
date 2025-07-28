# Simple Download Server (hdl_sv)

A lightweight, high-performance file download server written in Rust that offers secure, cross-platform file sharing with detailed logging and resilient error handling. Every component has been designed for clarity, reliability, and developer friendliness.

---

## 🚀 Key Features

- **Directory Listing** – Generates a styled HTML page showing file names, human-readable sizes, and last-modified timestamps
- **Secure File Downloads** – Streams large files efficiently, honours HTTP range requests, and limits downloads to allowed extensions with glob support (e.g., `*.zip`)
- **Path-Traversal Protection** – Canonicalises every request path and rejects any attempt that escapes the served directory
- **Optional Basic Authentication** – Username and password can be supplied via CLI flags; unauthenticated requests receive a 401 challenge
- **Multi-Threaded Core** – A configurable thread-pool accepts dozens of concurrent TCP connections without blocking the main listener
- **Rich Logging** – Each request is tagged with an eight-character ID and logged at `debug`, `info`, or `warn` level depending on CLI flags
- **Zero Non-Std Dependencies for Networking** – Networking is implemented using Rust's standard library, ensuring minimal attack surface

---

## 📋 Requirements

| Tool                    | Minimum Version | Purpose                   |
|-------------------------|-----------------|---------------------------|
| Rust                    | 1.88            | Compile the project       |
| Cargo                   | Comes with Rust | Dependency management     |
| Linux / macOS / Windows | –               | Runtime platform support |

---

## 🛠️ Installation

### Build from Source

```bash
# Clone the repository
git clone https://github.com/dev-harsh1998/SimpleDownloadServer.git
cd SimpleDownloadServer

# Build in release mode
cargo build --release
```

The resulting binary is `target/release/hdl_sv`; move it into any directory on your `$PATH`.

```bash
sudo mv target/release/hdl_sv /usr/local/bin/
```

### Windows

```powershell
move target\release\hdl_sv.exe C:\Tools\
```

---

## 🚦 Quick Start

Serve the current directory on the default port:

```bash
hdl_sv -d .
```

Open a browser at [http://127.0.0.1:8080](http://127.0.0.1:8080) and you will see the auto-generated directory index.

---

## 🎛️ Friendly CLI Reference

| Flag                 | Alias | Description                        | Default         |
|----------------------|-------|------------------------------------|-----------------|
| `--directory`        | `-d`  | Directory to serve (required)      | –               |
| `--listen`           | `-l`  | Bind address                       | `127.0.0.1`     |
| `--port`             | `-p`  | TCP port                           | `8080`          |
| `--allowed-extensions` | `-a`| Comma-separated glob patterns      | `*.zip,*.txt`   |
| `--threads`          | `-t`  | Thread-pool size                   | `8`             |
| `--chunk-size`       | `-c`  | File read buffer in bytes          | `1024`          |
| `--username`         | –     | Basic-auth user                    | none            |
| `--password`         | –     | Basic-auth password                | none            |
| `--verbose`          | `-v`  | Debug-level logs                   | `false`         |
| `--detailed-logging` | –     | Info-level logs                    | `false`         |

### Practical Examples

| Scenario                     | Command                                               |
|------------------------------|-------------------------------------------------------|
| Public share on port 3000   | `hdl_sv -d /srv/files -p 3000 -l 0.0.0.0`            |
| Allow only PDFs and images  | `hdl_sv -d ./docs -a "*.pdf,*.png,*.jpg"`            |
| High-throughput server       | `hdl_sv -d ./big -t 16 -c 8192`                      |
| Password-protected area      | `hdl_sv -d ./private --username alice --password s3cret` |

---

## 🏗️ Architecture Overview

The codebase is split into clear modules—`server.rs` handles the thread-pool listener, `http.rs` parses requests, `fs.rs` builds directory listings, and `response.rs` streams files or embedded error pages. Error types live in `error.rs`, while `cli.rs` defines the user-facing interface.

### System Architecture Flow

```
    +-------------------+       +------------------+       +-------------------+
    |   CLI Parser      | ----> |   Server Init    | ----> |   Thread Pool     |
    |   (cli.rs)        |       |   (main.rs)      |       |   (server.rs)     |
    +-------------------+       +------------------+       +-------------------+
                                                                      |
                                                                      v
    +-------------------+       +------------------+       +-------------------+
    |   File System     | <---- |   HTTP Handler   | <---- |  Request Router   |
    |   (fs.rs)         |       |  (response.rs)   |       |   (http.rs)       |
    +-------------------+       +------------------+       +-------------------+
```

### Request Processing Flow

```
                              HTTP Request
                                   |
                                   v
                        +---------------------+
                        |   Authentication    |  --[Fail]--> 401 Unauthorized
                        |       Check         |
                        +---------------------+
                                   | [Pass]
                                   v
                        +---------------------+
                        |    Path Safety      |  --[Fail]--> 403 Forbidden  
                        |       Check         |
                        +---------------------+
                                   | [Pass]
                                   v
                        +---------------------+
                        |   Resource Type     |
                        |     Detection       |
                        +---------------------+
                                   |
                    +--------------+---------------+
                    |                              |
                    v                              v
            [Directory]                        [File]
                    |                              |
                    v                              v
        Generate HTML Listing            Stream File Content
                                                  |
                                     +------------+------------+
                                     |                         |
                                     v                         v
                              [Range Request]           [Full Request]
                                     |                         |
                                     v                         v
                              Partial Content           Complete File
```

---

## 📦 Project Layout

```
src/
├── main.rs         # Entry point
├── lib.rs          # Logger + CLI bootstrap
├── cli.rs          # Command-line definitions
├── server.rs       # TCP listener + thread pool
├── http.rs         # HTTP parsing & routing
├── fs.rs           # Directory operations
├── response.rs     # Success & error responses
├── error.rs        # Custom error enum
└── utils.rs        # Helper utilities
tests/
└── integration_test.rs
assets/
├── error_400.dat
├── error_403.dat
└── error_404.dat
```

Every module is documented and formatted with `cargo fmt` and `clippy -- -D warnings` to keep technical debt at zero.

---

## 🧪 Testing

Run all integration tests—including range requests, auth success/failure, and 404 scenarios—with:

```bash
cargo test -- --nocapture
```

Tests start the server on a random port, issue real HTTP requests using `reqwest`, and verify both status codes and body integrity.

---

## 🛠️ Development

Developers can launch the server with live `debug` logs by exporting `RUST_LOG=debug` before running `cargo run`.

### Development Workflow

1. **Setup Development Environment**
   ```bash
   git clone https://github.com/dev-harsh1998/SimpleDownloadServer.git
   cd SimpleDownloadServer
   cargo build
   ```

2. **Run with Debug Logging**
   ```bash
   RUST_LOG=debug cargo run -- -d ./test-files -v
   ```

3. **Format and Lint**
   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   ```

4. **Run Tests**
   ```bash
   cargo test
   ```

---

## 👥 Contributors & Test Coverage Initiative

### Current Contributors

We're proud to acknowledge our contributors who have helped make SimpleDownloadServer a reliable and feature-rich project:

| Name              | GitHub Profile | Primary Contributions                            |
|-------------------|----------------|--------------------------------------------------|
| **Harshit Jain**  | [@dev-harsh1998](https://github.com/dev-harsh1998) | Project founder, core architecture, main development |
| **Sonu Kumar Saw** | [@dev-saw99](https://github.com/dev-saw99)         | Code improvements and enhancements              |

> **Want to see your name here?** We actively welcome new contributors! Your name will be added to this list after your first merged pull request.

### 🧪 **Test Coverage & Quality Initiative**

**We strongly believe that robust testing is the foundation of reliable software.** To maintain and improve the quality of SimpleDownloadServer, we have a special focus on test coverage and encourage all contributors to prioritize testing.

#### 🎯 **What We're Looking For:**

1. **Test Cases for New Features** - Every new feature or bug fix should include corresponding test cases
2. **Test Cases for Existing Code** - We welcome PRs that only add tests for existing functionality
3. **Integration Tests** - Tests that verify end-to-end functionality
4. **Edge Case Testing** - Tests that cover error conditions, boundary conditions, and security scenarios

#### 💡 **Easy Ways to Contribute:**

**For Code Contributors:**
- Add at least one test case for every PR you submit
- Include both positive and negative test scenarios
- Test error handling and edge cases
- Document your test strategy in the PR description

**For Test-Only Contributors:**
- Submit PRs that **only add test cases** for existing features
- Look for untested code paths in our current codebase
- Add regression tests for previously reported issues
- Improve test coverage for security features (authentication, path traversal protection)

#### **Current Testing Areas That Need Help:**

- Range request handling edge cases
- Authentication bypass attempts
- File extension filtering with complex glob patterns
- Error page generation under various conditions
- Concurrent connection stress testing
- Memory usage under high load

---

## 🤝 Contribution Guide

We love new ideas! Follow these simple steps to join the party:

### **Step-by-Step Process:**

1. **Fork** the repository and create your feature branch:
   ```bash
   git checkout -b feature/your-improvement
   # or for test-only contributions:
   git checkout -b tests/add-authentication-tests
   ```

2. **Make your changes** and **add tests** (this is crucial!):
   - For new features: implement both the feature and its tests
   - For test-only contributions: focus on comprehensive test coverage
   - For bug fixes: add a test that reproduces the bug, then fix it

3. **Run the full test suite** and formatting tools:
   ```bash
   cargo test
   cargo fmt && cargo clippy -- -D warnings
   ```

4. **Commit with descriptive messages:**
   ```bash
   git commit -m "feat: add timeout handling for downloads"
   # or
   git commit -m "test: add comprehensive tests for basic auth"
   ```

5. **Push and create a Pull Request:**
   ```bash
   git push origin feature/your-improvement
   ```

6. **In your PR description, please include:**
   - What changes you made
   - **What tests you added and why**
   - How to verify your changes work
   - Any edge cases you considered

### **PR Review Criteria:**

✅ **We prioritize PRs that include:**
- Comprehensive test coverage
- Clear documentation of test strategy
- Tests for both success and failure scenarios
- Integration tests where applicable

✅ **Special fast-track for:**
- Test-only contributions
- PRs that significantly improve test coverage
- Bug fixes with accompanying regression tests

### Developer Etiquette

- Be kind in code reviews—every improvement helps the project grow

### 🎉 **Get Started Today!**

Don't know where to start? Here are some **beginner-friendly test contributions:**

1. Add tests for CLI parameter validation
2. Test error message formatting
3. Add tests for directory listing HTML generation
4. Test file streaming with various file sizes
5. Add security tests for path traversal attempts

**Every test case counts!** Even if you can only add one test, it makes the project better for everyone.

---

## 📈 Performance Characteristics

- **Memory Usage**: ~2MB baseline + (thread_count × 8KB stack)
- **Concurrent Connections**: Limited by thread pool size (default: 8)
- **File Streaming**: Configurable chunk size (default: 1KB)
- **Request Latency**: <1ms for directory listings, variable for file downloads

---

## 🔒 Security Features

- **Path Traversal Prevention**: All paths are canonicalized and validated
- **Extension Filtering**: Only specified file types can be downloaded
- **Basic Authentication**: Optional username/password protection
- **Request Logging**: Every request is logged with unique IDs for auditing

---

## 📜 License

Simple Download Server is distributed under the **GPL-3.0** license; see `LICENSE` for details.

---

*Made with 🦀 in Bengaluru*