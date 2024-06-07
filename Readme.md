# Simple Configurable File Download Server

This lightweight Rust application creates a web server for sharing files from a directory of your choice. It generates a clean, user-friendly listing of the files and folders, and allows secure downloading of specific file types.


## Features

- **Directory Listing:**  Serves a styled HTML page showing the contents of the directory.
- **File Download:**  Enables direct download of files with configurable allowed extensions.
- **Streaming Downloads:**  Efficiently handles large file downloads.
- **No External Crates:** Uses only Rust's standard library for networking and file handling.

## Building

1.***Clone and build the Repository:***
```
git clone https://github.com/dev-harsh1998/SimpleDownloadServer.git

cd SimpleDownloadServer

cargo build --release
```

## Installing
2. ***Move generated bin a location present in user's path***

```
sudo mv target/release/hdl_sv /usr/bin/
or
mv target/release/hdl_sv /local/usr/bin
```

## Usage

```
Usage: hdl_sv [OPTIONS] --directory <DIRECTORY>

Options:
  -d, --directory <DIRECTORY>
          Directory path to serve, mandatory
  -l, --listen <LISTEN>
          Host address to listen on (e.g., "127.0.0.1", "0.0.0.0") [default: 127.0.0.1]
  -p, --port <PORT>
          Port number to listen on [default: 8080]
  -a, --allowed-extensions <ALLOWED_EXTENSIONS>
          Allowed file extensions for download (comma-separated) [default: zip,txt]
  -h, --help
          Print help (see more with '--help')
  -V, --version
          Print version
```

### Sample
> hdl_sv -d /home/user/directory/to/serve -a zip,img,txt -p 6969 -l 127.0.0.1

## Customization
You can modify the inline CSS in the generate_directory_listing function to change the appearance of the directory listing, moreover, you can also customize the error images in the asset directory just follow the same naming conventions.