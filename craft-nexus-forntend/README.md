# 🎨 CraftNexus Frontend

Welcome to the **CraftNexus** frontend application! This is a modern Next.js marketplace where artisans can sell handcrafted products and educational courses globally using Stellar's blockchain payments.

## 🚀 Getting Started

Welcome to CraftNexus! Follow these steps to get the frontend running locally and start contributing.

### 🛠️ Prerequisites

Before you begin, ensure you have these tools installed:

```bash
# Check Node.js (v18+ required)
node -v

# Check npm
npm -v

# Check Git
git --version
```

### 🚀 Quick Start

1. **Fork the Repository**
   - Visit https://github.com/EvolutionalHub/CraftNexus
   - Click the "Fork" button in the top right

2. **Clone Your Fork**
   ```bash
   git clone https://github.com/<your-github-username>/CraftNexus.git
   cd CraftNexus/craft-nexus
   ```

3. **Install Dependencies**
   ```bash
   npm install
   ```

4. **Configure Environment**
   ```bash
   # Create environment file
   cp .env.example .env.local
   
   # Edit with your configuration
   # Stellar Network Configuration
   NEXT_PUBLIC_STELLAR_NETWORK=TESTNET
   
   # Escrow Smart Contract Address (deploy contract first)
   NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=
   
   # Platform Commission Wallet (optional)
   NEXT_PUBLIC_PLATFORM_WALLET=
   ```

5. **Start Development Server**
   ```bash
   npm run dev
   ```

6. **Open in Browser**
   - Visit http://localhost:3000

7. **Connect Wallet**
   - Install [Freighter Wallet](https://freighter.app)
   - Fund with testnet XLM from [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test)
   - Connect your wallet in the CraftNexus app

---

## 🌿 Creating a Checkout Branch

Always work from a feature branch! This keeps the main branch clean and your changes organized.

### Branch Naming Convention

```bash
# Format: <type>:<description>
git checkout -b feat:add-user-profile
git checkout -b fix:wallet-connection-issue
git checkout -b docs:update-api-documentation
git checkout -b refactor:optimize-payment-flow
```

**Branch Types:**
- `feat:` - New features or improvements
- `fix:` - Bug fixes
- `refactor:` - Code refactoring
- `docs:` - Documentation updates

**Examples:**
- `feat:add-course-filter`
- `fix:navbar-mobile-responsive`
- `docs:contributor-guide`
- `refactor:stellar-payment-service`

---

## 📤 Submitting a Pull Request

### Step-by-Step PR Process

1. **Make Your Changes**
   ```bash
   # Add all changes
   git add .
   
   # Commit with clear message
   git commit -m "feat: add user profile page with avatar upload"
   
   # Push to your fork
   git push origin feat:add-user-profile
   ```

2. **Create Pull Request**
   - Go to your fork on GitHub
   - Click "New Pull Request"
   - Select your feature branch
   - Fill out the PR template

3. **PR Requirements**
   - ✅ **Clear description** of what you changed and why
   - ✅ **Screenshots/videos** for UI changes (required!)
   - ✅ **Test instructions** if applicable
   - ✅ **Follows code style** and conventions

4. **Proof Requirements**
   - **UI Changes**: Attach screenshots or screen recordings
   - **Bug Fixes**: Show before/after behavior
   - **New Features**: Demonstrate functionality

### ⭐ Important: Star Requirement

**All contributors must star the repository for PR approval!**

- Click the ⭐ star button at https://github.com/EvolutionalHub/CraftNexus
- This helps the project grow and shows your support

---

## 🆘 Need Help? Contact Maintainers

### Get Support

**🔥 Click here to join our Telegram group:**
https://t.me/c/2334943542/1

**What to expect in the Telegram group:**
- 🚀 Quick code reviews and feedback
- 💬 Technical discussions and help
- 📢 Project updates and announcements
- 🤝 Collaboration opportunities

### When to Contact Maintainers

- **PR Review**: Your PR is ready for review
- **Technical Issues**: Blocked by bugs or unclear requirements
- **Feature Questions**: Need clarification on implementation
- **Setup Problems**: Having trouble with local development

### Best Practices

- **Be Specific**: Include error messages, screenshots, and steps to reproduce
- **Stay Patient**: Maintainers will respond as soon as possible
- **Be Constructive**: Help others while waiting for your own review

---

## 🛠️ Development Guide

### Project Structure

```
craft-nexus/
├── app/                    # Next.js app router
│   ├── (auth)/            # Authentication routes
│   ├── marketplace/       # Marketplace pages
│   ├── dashboard/         # User dashboard
│   └── globals.css        # Global styles
├── components/            # Reusable React components
│   ├── ui/               # Base UI components
│   ├── forms/            # Form components
│   └── layout/           # Layout components
├── lib/                  # Utility libraries
│   ├── stellar/          # Stellar blockchain integration
│   ├── utils/            # Helper functions
│   └── types/            # TypeScript definitions
├── hooks/                # Custom React hooks
├── public/               # Static assets
└── scripts/              # Build and deployment scripts
```

### Key Technologies

- **Frontend**: Next.js 15, React 19, TypeScript
- **Styling**: Tailwind CSS, shadcn/ui components
- **Blockchain**: Stellar SDK, Freighter Wallet API
- **State Management**: React Context, Zustand
- **Forms**: React Hook Form, Zod validation

### Common Development Tasks

**Adding a New Page:**
```bash
# Create new route in app/ directory
mkdir app/new-feature
touch app/new-feature/page.tsx
```

**Adding Components:**
```bash
# Create new component
mkdir components/features/new-feature
touch components/features/new-feature/ComponentName.tsx
```

**Stellar Integration:**
```typescript
import { connectWallet, makePayment } from '@/lib/stellar/wallet';

// Connect to Freighter wallet
const wallet = await connectWallet();

// Make USDC payment
const payment = await makePayment({
  to: 'recipient-address',
  amount: '10',
  currency: 'USDC'
});
```

---

## 🤝 Contribution Guidelines

### Code Quality

- Follow existing code style and patterns
- Write clean, commented code
- Test your changes thoroughly
- Update documentation when needed

### UI/UX Standards

- Use shadcn/ui components when possible
- Follow responsive design principles
- Ensure accessibility (ARIA labels, keyboard navigation)
- Test on different screen sizes

### Git Best Practices

- Write clear, descriptive commit messages
- Keep PRs focused and manageable
- Include tests for new features
- Update documentation for API changes

---

## 🌟 Support the Project

If you find CraftNexus valuable:

- ⭐ **Star the repository** (required for contributors!)
- 🔄 **Share with your network**
- 💬 **Tell others about the project**
- 🤝 **Contribute code or documentation**

Your support helps us:
- Attract more contributors
- Secure funding and grants
- Build a stronger community
- Empower artisans globally

---

## 📚 Additional Resources

- [Next.js Documentation](https://nextjs.org/docs)
- [Stellar Developer Documentation](https://developers.stellar.org/)
- [Freighter Wallet Documentation](https://freighter.app/docs)
- [Tailwind CSS Documentation](https://tailwindcss.com/docs)
- [shadcn/ui Components](https://ui.shadcn.com/)

---

*Thank you for contributing to CraftNexus! Together, we're building a more inclusive global marketplace for creators.*
