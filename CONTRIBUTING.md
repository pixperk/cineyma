# Contributing to Cineyma

Thank you for your interest in contributing to Cineyma! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites

- Rust 1.75 or later
- Cargo (comes with Rust)
- Git

### Setting Up the Development Environment

1. Fork the repository on GitHub
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/cineyma.git
   cd cineyma
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/pixperk/cineyma.git
   ```
4. Build the project:
   ```bash
   cargo build
   ```
5. Run tests:
   ```bash
   cargo test
   ```

## Development Workflow

### Branching Strategy

- `main` - stable release branch
- Feature branches should be created from `main`
- Use descriptive branch names: `feature/add-sharding`, `fix/mailbox-overflow`, `docs/update-readme`

### Making Changes

1. Create a new branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```
2. Make your changes
3. Run tests and ensure they pass:
   ```bash
   cargo test
   ```
4. Run clippy for linting:
   ```bash
   cargo clippy -- -D warnings
   ```
5. Format your code:
   ```bash
   cargo fmt
   ```
6. Commit your changes with a descriptive message

### Commit Messages

We follow conventional commit messages:

- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `test:` - Adding or updating tests
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

Examples:
```
feat: add actor sharding support
fix: resolve mailbox overflow on high throughput
docs: update quickstart guide with async handlers
```

### Pull Requests

1. Push your branch to your fork:
   ```bash
   git push origin feature/your-feature-name
   ```
2. Open a Pull Request against `main`
3. Fill out the PR template
4. Wait for review and address any feedback

#### PR Requirements

- All tests must pass
- Code must be formatted with `cargo fmt`
- No clippy warnings
- Documentation for public APIs
- Tests for new functionality

## Code Style

### General Guidelines

- Follow Rust idioms and best practices
- Use meaningful variable and function names
- Keep functions focused and small
- Document public APIs with doc comments
- Add inline comments for complex logic

### Documentation

All public items should have documentation:

```rust
/// Creates a new actor system with default configuration.
///
/// # Examples
///
/// ```
/// use cineyma::ActorSystem;
///
/// let system = ActorSystem::new();
/// ```
pub fn new() -> Self {
    // ...
}
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run benchmarks
cargo bench
```

### Writing Tests

- Place unit tests in the same file as the code they test
- Place integration tests in the `tests/` directory
- Use descriptive test names
- Test both success and failure cases

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_receives_message() {
        // Test implementation
    }
}
```

## Reporting Issues

### Bug Reports

When reporting bugs, please include:

- Cineyma version
- Rust version (`rustc --version`)
- Operating system
- Minimal reproduction code
- Expected vs actual behavior
- Any error messages or logs

### Feature Requests

For feature requests, please describe:

- The problem you're trying to solve
- Your proposed solution
- Any alternatives you've considered
- Potential impact on existing functionality

## Community

- [GitHub Issues](https://github.com/pixperk/cineyma/issues) - Bug reports and feature requests
- [GitHub Discussions](https://github.com/pixperk/cineyma/discussions) - Questions and general discussion

## License

By contributing to Cineyma, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing to Cineyma! 🎬
