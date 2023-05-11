# DBConnect
DBConnect is a command-line tool that allows you to connect to different database instances with ease.

## Installation
To install DBConnect from source, follow these steps:
- Install Rust from https://www.rust-lang.org/tools/install
- Clone this repository and navigate to the project directory
- Run `cargo build --release` to compile the binary
- Run `sudo cp target/release/dbconnect /usr/bin` to copy the binary to your system path

## Usage
To use DBConnect, run the following commands:
- dbconnect -l to list all the available environments you can connect to
- dbconnect -e <env> to connect to a specific environment using one of the listed options

## Contribution
If you want to contribute to this project, please follow these guidelines:
- Fork this repository and create a new branch for your feature or bugfix
- Write clear and concise commit messages and pull request descriptions
- Follow the code style and formatting conventions of the project
- Add tests and documentation for your changes
- Submit your pull request and wait for feedback
## License
This project is licensed under the MIT License. See the [LICENSE](./LICENSE) file for more details.

