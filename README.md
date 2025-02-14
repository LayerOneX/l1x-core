# L1X-Core

Primary repository for the L1X blockchain protocol, encompassing its core infrastructure.

## Description
This repository forms the backbone of the Layer One X network. It contains the core implementation of L1X’s protocol, including consensus algorithms, validator configurations, and network interactions. Ideal for contributors looking to work on the protocol-level advancements of L1X.

## Table of Contents
- [Description](#description)
- [Installation](#installation)
- [Usage](#usage)
- [Contributing](#contributing)
- [Roadmap](#roadmap)
- [License](#license)
- [Acknowledgments](#acknowledgments)
- [Contact](#contact)
- [Code of Conduct](#code-of-conduct)

## Installation

### Prerequisites
- Docker v20.10.x or higher
- Docker Compose v2.x.x
- Git
- 4GB RAM minimum (8GB recommended)
- 50GB available disk space


### Database Setup

L1X Core uses PostgreSQL as its primary database system for storing blockchain data and maintaining node state.

Using Docker and Docker Compose:

The default database credentials are:
- Username: `admin`
- Password: `admin`

These credentials are configured in the `docker-compose.yml` file. For production environments, it's recommended to:
1. Change these default credentials
2. Use environment variables or a secure secrets management system
3. Never commit production credentials to version control

The database configuration file `l1x_core_db.conf` includes the following default settings:
- Port: 5433
- Maximum connections: 100

These settings can be adjusted based on your deployment requirements and hardware specifications.

### Building from Source

Using Docker and Docker Compose:

```
chmod +x ./run_l1x_core_node.sh
chmod +x ./docker-compose-entrypoint.sh
```

```
docker build -f Dockerfile-build -t l1x-v2-compiler .
```

## Usage

### Configuring the Node

- Copy the `config.toml.example` file to `config.toml`
- Update the `config.toml` file with your desired settings

```
cp config.toml.example config.toml
```

More details about the `config.toml` file can be found in the [config.toml.example](config.toml.example) file.

### Running a Full Node

Using Docker and Docker Compose:

```
./docker-compose-entrypoint.sh
```


## Contributing
We welcome contributions from the community! Please see our [CONTRIBUTING.md](Contributing.md) for detailed guidelines on:
- Code style and standards
- Pull request process
- Issue reporting
- Development workflow

## Roadmap
Our development roadmap focuses on the following key milestones: https://quick.l1xapp.com/L1XRoadMap2025

## License

This project is licensed under the Perimeter License which allows any kind of use of the underlying code, but explicitly states the following: “Any purpose is a permitted purpose, except for providing to others any product that competes with the software. If you use this software to market a product as a substitute for the functionality or value of the software, it competes with the software. A product may compete regardless how it is designed or deployed. For example, a product may compete even if it provides its functionality via any kind of interface (including services, libraries or plug-ins), even if it is ported to a different platforms or programming languages, and even if it is provided free of charge.”

https://polyformproject.org/
https://polyformproject.org/licenses/perimeter/1.0.0/
https://github.com/polyformproject/polyform-licenses

## Acknowledgments
Special thanks to:
- The L1X Community
- The L1X Foundation team


## Contact
- Technical Support: https://quick.l1xapp.com/L1XDiscordDevSupport
- Email: devs@l1x.foundation
- Bug Reports: GitHub Issues
- Bug Bounty Program: To report any security issues, please contact us at devs@l1x.foundation

## Code of Conduct

The L1X Foundation is committed to fostering a friendly, safe, and inclusive environment for all participants in our ecosystem. We welcome individuals of all genders, sexual orientations, abilities, ethnicities, religions, and other personal characteristics to contribute and collaborate freely.

### Our Standards

To foster a positive and inclusive environment, we expect community members to:
- Use welcoming and inclusive language.
- Respect differing viewpoints and experiences.
- Accept constructive criticism gracefully.
- Prioritize the best interests of the L1X ecosystem.
- Show empathy and kindness towards other community members.

Unacceptable behavior includes, but is not limited to:
- Using sexualized language or imagery and making unwelcome sexual attention or advances.
- Trolling, insulting, or making derogatory comments, as well as engaging in personal or political attacks.
- Harassing others, either publicly or privately.
- Sharing someone’s private information (e.g., physical or electronic addresses) without their explicit permission.
- Engaging in any other conduct deemed inappropriate in a professional environment.

### Enforcement

The L1X Foundation reserves the sole discretion to interpret and enforce this Code of Conduct. Project maintainers and Foundation representatives may take any action deemed necessary, including temporary or permanent removal from project spaces, to address behavior that is deemed inappropriate, threatening, offensive, or harmful.

Reports of abusive, harassing, or otherwise unacceptable behavior can be submitted to the L1X Foundation at devs@l1x.foundation. All complaints will be thoroughly reviewed and investigated, with actions taken as necessary and appropriate based on the circumstances. The Foundation is committed to maintaining confidentiality regarding the identity of the reporter and ensuring a fair and safe resolution process.

### Scope

This Code of Conduct applies to all spaces managed by the L1X Foundation, including but not limited to:
- Code repositories
- Documentation platforms
- Community forums and chat rooms
- Social media channels
- Events, meetups, and other gatherings
- Any other official L1X Foundation-managed platforms or environments

Participants are expected to adhere to this Code of Conduct in all such spaces and in interactions related to the L1X ecosystem.

### Updates and Amendments

The L1X Foundation retains the right to update or amend this Code of Conduct at its sole discretion. Any significant changes will be communicated to the community in a timely manner to ensure transparency and continued adherence.
