import Image from "next/image";
import LoginForm from "@/components/sell/LoginForm";

export default function ArtisanLoginPage() {
  return (
    <main className="artisan-login-page">
      {/* Left Panel - Illustration & Branding (Desktop) */}
      <div className="artisan-login-left">
        {/* Decorative background elements */}
        <div className="artisan-login-decor artisan-login-decor--2" />
        <div className="artisan-login-decor artisan-login-decor--3" />

        {/* Logo */}
        <div className="artisan-login-logo">
          <div className="artisan-login-logo-icon">
            <Image
              src="/logo.svg"
              alt="Craft Nexus Logo"
              width={123}
              height={77}
              className="object-contain"
            />
          
          </div>
        </div>

        {/* Center Content */}
        <div className="artisan-login-left-content">
          {/* Illustration */}
          <div className="artisan-login-illustration">
            <Image
              src="/illustration/artisan-work.svg"
              alt="Pottery Artisan"
              width={320}
              height={320}
              className="artisan-login-illustration-img"
              priority
            />
          </div>

          {/* Text */}
          <div className="artisan-login-left-text">
            <p className="artisan-login-portal-label">Artisan Portal</p>
            <h1 className="artisan-login-hero-title">
              Welcome Back,
              <br />
              Creator
            </h1>
            <p className="artisan-login-hero-subtitle">
              Sign in to manage your courses, artworks, and sales.
            </p>
          </div>
        </div>
      </div>

      {/* Right Panel - Login Form */}
      <div className="artisan-login-right">
        {/* Mobile Logo */}
        <div className="artisan-login-mobile-logo">
          <div className="artisan-login-mobile-logo-icon">
            <Image
              src="/logo.svg"
              alt="Craft Nexus Logo"
              width={24}
              height={24}
              className="object-contain"
            />
          </div>
          <span className="artisan-login-mobile-logo-text">CRAFT NEXUS</span>
        </div>

        <LoginForm />
      </div>

      <style>{`
        .artisan-login-page {
          min-height: 100svh;
          display: flex;
          width: 100%;
        }

        /* ===== Left Panel ===== */
        .artisan-login-left {
          display: none;
          flex-direction: column;
          width: 50%;
          background: #FDF8F5;
          position: relative;
          overflow: hidden;
        }

        /* Decorative circles */
        .artisan-login-decor {
          position: absolute;
          border-radius: 50%;
          z-index: 0;
        }

       

        .artisan-login-decor--2 {
          width: 200px;
          height: 200px;
          background: rgba(107, 144, 128, 0.06);
          bottom: 8%;
          right: 10%;
        }

        .artisan-login-decor--3 {
          width: 96px;
          height: 96px;
          background: rgba(196, 146, 143, 0.08);
          top: 50%;
          left: 38%;
        }

        /* Logo */
        .artisan-login-logo {
          position: absolute;
          top: 40px;
          left: 40px;
          z-index: 10;
          display: flex;
          align-items: center;
          gap: 10px;
        }

        .artisan-login-logo-icon {
          width: 128px;
          height: 128px;
          background:  #C4928F1A;
          border-radius: 50%;
          display: flex;
          align-items: center;
          justify-content: center;
          box-shadow: 0 2px 8px rgba(0, 0, 0, 0.06);
        }

       
        /* Center Content */
        .artisan-login-left-content {
          flex: 1;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          z-index: 1;
          padding: 80px 48px 48px;
        }

        .artisan-login-illustration {
          position: relative;
          margin-bottom: 40px;
        }

        .artisan-login-illustration-img {
          position: relative;
          z-index: 1;
          filter: drop-shadow(0 8px 24px rgba(0, 0, 0, 0.08));
        }

        .artisan-login-left-text {
          text-align: center;
        }

        .artisan-login-portal-label {
          font-family: var(--font-poppins), sans-serif;
          font-weight: 600;
          font-size: 12px;
          letter-spacing: 0.2em;
          text-transform: uppercase;
          color: #6B9080;
          margin: 0 0 16px;
        }

        .artisan-login-hero-title {
          font-family: var(--font-poppins), sans-serif;
          font-weight: 700;
          font-size: 44px;
          line-height: 1.15;
          color: #111827;
          margin: 0 0 16px;
          letter-spacing: -0.5px;
        }

        .artisan-login-hero-subtitle {
          font-family: var(--font-poppins), sans-serif;
          font-size: 16px;
          color: #6B7280;
          margin: 0;
          max-width: 600px;
          line-height: 1.6;
        }

        /* ===== Right Panel ===== */
        .artisan-login-right {
          width: 100%;
          display: flex;
          align-items: center;
          justify-content: center;
          background: #FFFFFF;
          padding: 24px;
          position: relative;
        }

        /* Mobile Logo */
        .artisan-login-mobile-logo {
          position: absolute;
          top: 28px;
          left: 28px;
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .artisan-login-mobile-logo-icon {
          width: 36px;
          height: 36px;
          background: #FDF8F5;
          border-radius: 50%;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .artisan-login-mobile-logo-text {
          font-family: var(--font-poppins), sans-serif;
          font-weight: 700;
          font-size: 11px;
          letter-spacing: 0.15em;
          color: #111827;
        }

        /* ===== Desktop Breakpoint ===== */
        @media (min-width: 1024px) {
          .artisan-login-left {
            display: flex;
          }

          .artisan-login-right {
            width: 50%;
            padding: 48px;
          }

          .artisan-login-mobile-logo {
            display: none;
          }
        }

        /* ===== Large Desktop ===== */
        @media (min-width: 1280px) {
          .artisan-login-hero-title {
            font-size: 52px;
          }

          .artisan-login-hero-subtitle {
            font-size: 17px;
          }
        }

        /* ===== Small Mobile ===== */
        @media (max-width: 640px) {
          .artisan-login-right {
            padding: 20px 16px;
          }

          .artisan-login-mobile-logo {
            top: 20px;
            left: 20px;
          }
        }
      `}</style>
    </main>
  );
}
