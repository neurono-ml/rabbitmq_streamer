# CI/CD Pipeline

This repository uses GitHub Actions for continuous integration and deployment. The pipeline is designed to automatically test code, create releases, and publish to crates.io.

## Workflows Overview

### 1. `ci.yml` - Continuous Integration
**Triggers:** Push to `main` or `develop`, Pull Requests
- ✅ Code formatting check (`cargo fmt`)
- ✅ Linting with Clippy
- ✅ Build verification
- ✅ Run all tests (including testcontainers)
- ✅ Security audit

### 2. `pr-test.yml` - Pull Request Testing
**Triggers:** Pull Request events (opened, synchronized, etc.)
- 🧪 Comprehensive test suite
- 📊 Code coverage analysis
- 📖 Documentation checks
- 🔒 Security auditing
- 📋 Test result summaries

### 3. `release.yml` - Automated Release Pipeline
**Triggers:** Push to `main` branch (when version changes)

**Process:**
1. **Version Detection** - Automatically detects version changes in `Cargo.toml`
2. **Testing** - Runs full test suite
3. **Git Tagging** - Creates annotated git tags (e.g., `v1.0.0`)
4. **GitHub Release** - Creates release with changelog
5. **Crates.io Publishing** - Automatically publishes to crates.io

### 4. `manual-release.yml` - Manual Release Workflow
**Triggers:** Manual workflow dispatch

Allows maintainers to:
- Specify version number
- Choose whether to create git tags
- Choose whether to publish to crates.io
- Update `Cargo.toml` version automatically

### 5. `maintenance.yml` - Weekly Maintenance
**Triggers:** Weekly schedule (Mondays) + Manual trigger
- 🔍 Dependency outdated check
- 🛡️ Security vulnerability scanning
- 🧪 Multi-version Rust testing (stable, beta, nightly)

## Required Secrets

The following secrets must be configured in the repository:

### `CARGO_REGISTRY_TOKEN`
- **Purpose:** Publishing to crates.io
- **How to get:** 
  1. Go to [crates.io](https://crates.io)
  2. Login and go to Account Settings
  3. Generate a new API token
  4. Add to GitHub repository secrets

### `GITHUB_TOKEN`
- **Purpose:** Creating releases and pushing tags
- **Note:** Automatically provided by GitHub Actions

## Automatic Release Process

When you want to release a new version:

### Method 1: Automatic (Recommended)
1. Update the version in `Cargo.toml`
2. Commit and push to `main` branch
3. The pipeline automatically:
   - Detects version change
   - Runs tests
   - Creates git tag
   - Creates GitHub release
   - Publishes to crates.io

### Method 2: Manual Release
1. Go to GitHub Actions tab
2. Select "Manual Release" workflow
3. Click "Run workflow"
4. Enter version number and options
5. The pipeline handles the rest

## Testing Strategy

The pipeline uses **testcontainers** to run integration tests with real RabbitMQ instances:

- **Publisher Tests** - Test message publishing functionality
- **Consumer Tests** - Test message consumption with auto/manual ack
- **Integration Tests** - End-to-end publisher-consumer scenarios

### Test Environment
- Uses Docker containers for RabbitMQ
- Isolated test environments
- Comprehensive error handling testing
- Multiple routing key scenarios

## Branch Protection

Recommended branch protection rules for `main`:

- ✅ Require status checks to pass
- ✅ Require branches to be up to date
- ✅ Require pull request reviews
- ✅ Dismiss stale reviews
- ✅ Restrict pushes to main

## Monitoring and Notifications

The pipeline provides:
- 📊 Test result summaries in PR comments
- 🏷️ Automatic version tagging
- 📦 Release notifications
- ⚠️ Security vulnerability alerts
- 📈 Code coverage reports

## Troubleshooting

### Common Issues

**Tests fail with Docker errors:**
- Ensure GitHub Actions has Docker available
- Check testcontainers configuration

**Publishing fails:**
- Verify `CARGO_REGISTRY_TOKEN` is valid
- Check if version already exists on crates.io

**Version detection issues:**
- Ensure `Cargo.toml` version follows semantic versioning
- Check that version actually changed from previous commit

### Manual Intervention

If automatic processes fail:
1. Check GitHub Actions logs
2. Use manual release workflow as fallback
3. Verify all secrets are properly configured
4. Ensure branch protection rules allow the action

## Performance Optimizations

The workflows include several optimizations:
- **Cargo caching** - Speeds up builds
- **Parallel job execution** - Faster overall pipeline
- **Conditional execution** - Only runs when needed
- **Incremental testing** - Focused test execution