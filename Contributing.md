Contributing to L1X Core

Thank you for your interest in contributing to the L1X Core repository! By participating in this project, you’re helping to build the future of fourth-generation blockchain interoperability. Follow the guidelines below to make your contributions effective and efficient.

🚀 Getting Started

Prerequisites

Before you begin contributing, ensure you have the following:
	•	A GitHub account
	•	Node.js (v18 or higher)
	•	Cargo/Rust installed (for CLI and VM development)
	•	Basic familiarity with blockchain development, consensus mechanisms, or smart contract deployment is recommended.

Fork & Clone the Repository
	1.	Fork the repository to your GitHub account.
	2.	Clone your forked repository:

git clone https://github.com/<your-username>/l1x-core.git
cd l1x-core


	3.	Add the main repository as an upstream remote:

git remote add upstream https://github.com/LayerOneX/l1x-core.git

🛠️ Contribution Workflow

Follow these steps to make your contributions:

1️⃣ Create a Branch

Before making changes, create a new branch for your work:

git checkout -b <feature/bugfix-name>

2️⃣ Make Your Changes

Contribute to one of the key areas of L1X Core:
	•	Consensus Mechanism: Work on our PoX consensus module.
	•	Networking: Improve blockchain communication protocols.
	•	EVM and VM Enhancements: Contribute to compatibility or introduce WASM improvements.
	•	Bug Fixes and Documentation: Fix bugs or enhance project documentation.

3️⃣ Test Your Changes

Run tests to ensure your contributions do not break existing functionality:

cargo test

For EVM-specific changes, refer to the L1X EVM Testing Guide.

4️⃣ Commit Your Changes

Follow our commit message style:

git commit -m "[Area]: Short description of your changes"

For example:

git commit -m "[Consensus]: Improved PoX block validation logic"

5️⃣ Push to Your Fork

git push origin <feature/bugfix-name>

6️⃣ Create a Pull Request (PR)

Submit a PR to the main branch of the repository. Include:
	•	A clear description of your changes
	•	Any relevant issue numbers (e.g., Closes #123)

✨ Code Style Guidelines
	1.	Follow Rust’s standard coding practices.
	2.	Adhere to the CONTRIBUTING.md in all core components like CLI, SDK, and RPC.
	3.	Use clear and concise comments.

🧪 Testing Guidelines

We prioritize well-tested code. Ensure the following:
	•	Write tests for new features or fixes.
	•	Run existing tests to verify integrity:

cargo test


	•	Add tests to the appropriate testing folder for your changes.

📢 Communication Channels

Join our community to ask questions, share ideas, or discuss issues:
	•	Discord: L1X Developer Community
	•	X (Twitter): @LayerOneX

🔒 Code of Conduct

We follow the L1X Code of Conduct. By contributing, you agree to adhere to these guidelines.

📄 Licensing

All contributions are covered under the MIT License.

By contributing, you’re directly impacting the L1X ecosystem and helping us build the future of blockchain interoperability. Thank you for being a part of our journey! 💪
