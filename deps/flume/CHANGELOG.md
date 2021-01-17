# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# Unreleased

### Added

### Removed

### Changed

### Fixed

# [0.10.1] - 2020-12-30

### Removed

- Removed `T: Unpin` requirement from async traits using `pin_project`

# [0.10.0] - 2020-12-09

### Changed

- Renamed `SendFuture` to `SendFut` to be consistent with `RecvFut`
- Improved async-related documentation

### Fixed

- Updated `nanorand` to address security advisory
