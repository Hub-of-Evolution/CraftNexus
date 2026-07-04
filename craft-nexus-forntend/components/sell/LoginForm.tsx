"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { FcGoogle } from "react-icons/fc";
import {
  IoWalletOutline,
  IoEyeOutline,
  IoEyeOffOutline,
} from "react-icons/io5";
import Link from "next/link";

export default function LoginForm() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [rememberMe, setRememberMe] = useState(false);
  const [errors, setErrors] = useState<{
    email?: string;
    password?: string;
    general?: string;
  }>({});
  const [isLoading, setIsLoading] = useState(false);

  const validate = () => {
    const newErrors: { email?: string; password?: string } = {};
    if (!email) {
      newErrors.email = "Email is required";
    } else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      newErrors.email = "Please enter a valid email address";
    }
    if (!password) {
      newErrors.password = "Password is required";
    }
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    setIsLoading(true);
    setErrors({});

    try {
      // Simulate authentication API call
      await new Promise((resolve) => setTimeout(resolve, 1200));
      router.push("/sell/dashboard");
    } catch {
      setErrors({ general: "Invalid email or password. Please try again." });
      setIsLoading(false);
    }
  };

  return (
    <div className="login-form-card">
      {/* Header */}
      <div className="login-form-header">
        <h1 className="login-form-title">Sign In</h1>
        <p className="login-form-subtitle">
          Welcome back to your artisan portal
        </p>
      </div>

      {/* General error message */}
      {errors.general && (
        <div className="login-form-error-banner">
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              d="M8 1C4.13 1 1 4.13 1 8s3.13 7 7 7 7-3.13 7-7-3.13-7-7-7zm.5 10.5h-1v-1h1v1zm0-2h-1v-5h1v5z"
              fill="#DC2626"
            />
          </svg>
          <span>{errors.general}</span>
        </div>
      )}

      <form onSubmit={handleSubmit} className="login-form-fields">
        {/* Email Field */}
        <div className="login-form-field">
          <label className="login-form-label" htmlFor="email">
            Email Address
          </label>
          <input
            id="email"
            type="email"
            placeholder="Enter your email"
            className={`login-form-input ${errors.email ? "login-form-input--error" : ""}`}
            value={email}
            onChange={(e) => {
              setEmail(e.target.value);
              if (errors.email)
                setErrors((prev) => ({ ...prev, email: undefined }));
            }}
            disabled={isLoading}
            autoComplete="email"
          />
          {errors.email && (
            <p className="login-form-field-error">{errors.email}</p>
          )}
        </div>

        {/* Password Field */}
        <div className="login-form-field">
          <label className="login-form-label" htmlFor="password">
            Password
          </label>
          <div className="login-form-input-wrapper">
            <input
              id="password"
              type={showPassword ? "text" : "password"}
              placeholder="Enter your password"
              className={`login-form-input login-form-input--has-icon ${errors.password ? "login-form-input--error" : ""}`}
              value={password}
              onChange={(e) => {
                setPassword(e.target.value);
                if (errors.password)
                  setErrors((prev) => ({ ...prev, password: undefined }));
              }}
              disabled={isLoading}
              autoComplete="current-password"
            />
            <button
              type="button"
              className="login-form-eye-btn"
              onClick={() => setShowPassword(!showPassword)}
              aria-label={showPassword ? "Hide password" : "Show password"}
              tabIndex={-1}
            >
              {showPassword ? (
                <IoEyeOffOutline size={18} />
              ) : (
                <IoEyeOutline size={18} />
              )}
            </button>
          </div>
          {errors.password && (
            <p className="login-form-field-error">{errors.password}</p>
          )}
        </div>

        {/* Remember Me & Forgot Password */}
        <div className="login-form-options">
          <label className="login-form-checkbox-label">
            <input
              type="checkbox"
              className="login-form-checkbox"
              checked={rememberMe}
              onChange={(e) => setRememberMe(e.target.checked)}
              disabled={isLoading}
            />
            <span>Remember Me</span>
          </label>
          <Link href="/sell/forgot-password" className="login-form-forgot-link">
            Forgot Password?
          </Link>
        </div>

        {/* Submit Button */}
        <button
          type="submit"
          className="login-form-submit-btn"
          disabled={isLoading}
        >
          {isLoading ? <div className="login-form-spinner" /> : "Sign In"}
        </button>
      </form>

      {/* OR Divider */}
      <div className="login-form-divider">
        <div className="login-form-divider-line" />
        <span className="login-form-divider-text">OR</span>
        <div className="login-form-divider-line" />
      </div>

      {/* Social Login Buttons */}
      <div className="login-form-social">
        <button
          type="button"
          className="login-form-social-btn"
          disabled={isLoading}
        >
          <FcGoogle size={20} />
          <span>Continue with Google</span>
        </button>
        <button
          type="button"
          className="login-form-social-btn"
          disabled={isLoading}
        >
          <IoWalletOutline size={20} />
          <span>Continue with Wallet</span>
        </button>
      </div>

      {/* Register Link */}
      <p className="login-form-register">
        Don&apos;t have an account?
        <br/>
          <Link href="/sell/onboarding" className="login-form-register-link">
          Register as an Artisan
        </Link>
        
        
      </p>

      <style>{`
        .login-form-card {
          width: 100%;
          max-width: 420px;
          padding: 40px;
          background: #FFFFFF;
          border-radius: 12px;
          box-shadow: 0 10px 40px -10px rgba(0, 0, 0, 0.15);
        }

        .login-form-header {
          text-align: center;
          margin-bottom: 32px;
        }

        .login-form-title {
          font-family: var(--font-poppins), sans-serif;
          font-size: 32px;
          font-weight: 700;
          color: #111827;
          margin: 0 0 8px;
          letter-spacing: -0.5px;
          line-height: 1.2;
        }

        .login-form-subtitle {
          font-family: var(--font-poppins), sans-serif;
          font-size: 15px;
          color: #6B7280;
          margin: 0;
          line-height: 1.5;
        }

        /* Error Banner */
        .login-form-error-banner {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 12px 16px;
          background: #FEF2F2;
          border: 1px solid #FECACA;
          border-radius: 12px;
          margin-bottom: 24px;
          font-family: var(--font-poppins), sans-serif;
          font-size: 13px;
          color: #DC2626;
        }

        /* Form */
        .login-form-fields {
          display: flex;
          flex-direction: column;
          gap: 20px;
        }

        .login-form-field {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .login-form-label {
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          font-weight: 500;
          color: #374151;
        }

        .login-form-input {
          width: 100%;
          height: 48px;
          padding: 0 16px;
          border: 1.5px solid #E5E7EB;
          border-radius: 12px;
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          color: #111827;
          background: #FFFFFF;
          transition: border-color 0.2s, box-shadow 0.2s;
          outline: none;
          box-sizing: border-box;
        }

        .login-form-input::placeholder {
          color: #9CA3AF;
        }

        .login-form-input:focus {
          border-color: #C4928F;
          box-shadow: 0 0 0 3px rgba(196, 146, 143, 0.15);
        }

        .login-form-input--error {
          border-color: #EF4444;
        }

        .login-form-input--error:focus {
          border-color: #EF4444;
          box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.15);
        }

        .login-form-input:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }

        /* Password input wrapper */
        .login-form-input-wrapper {
          position: relative;
        }

        .login-form-input--has-icon {
          padding-right: 44px;
        }

        .login-form-eye-btn {
          position: absolute;
          right: 14px;
          top: 50%;
          transform: translateY(-50%);
          background: none;
          border: none;
          cursor: pointer;
          color: #9CA3AF;
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 4px;
          border-radius: 4px;
          transition: color 0.15s;
        }

        .login-form-eye-btn:hover {
          color: #6B7280;
        }

        /* Field Error */
        .login-form-field-error {
          font-family: var(--font-poppins), sans-serif;
          font-size: 12px;
          color: #EF4444;
          margin: 0;
        }

        /* Options Row */
        .login-form-options {
          display: flex;
          align-items: center;
          justify-content: space-between;
        }

        .login-form-checkbox-label {
          display: flex;
          align-items: center;
          gap: 8px;
          cursor: pointer;
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          color: #6B7280;
        }

        .login-form-checkbox {
          width: 16px;
          height: 16px;
          border-radius: 4px;
          border: 1.5px solid #D1D5DB;
          accent-color: #C4928F;
          cursor: pointer;
        }

        .login-form-forgot-link {
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          font-weight: 200;
          color: #517A77; 
          text-decoration: none;
          transition: color 0.15s;
        }

        .login-form-forgot-link:hover {
          color: #527063;
        }

        /* Submit Button */
        .login-form-submit-btn {
          width: 100%;
          height: 52px;
          background: #C4928F;
          color: #FFFFFF;
          border: none;
          border-radius: 14px;
          font-family: var(--font-outfit), sans-serif;
          font-size: 16px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.2s;
          display: flex;
          align-items: center;
          justify-content: center;
          margin-top: 4px;
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08), 0 4px 12px rgba(196, 146, 143, 0.2);
        }

        .login-form-submit-btn:hover:not(:disabled) {
          background: #b07c76;
          transform: translateY(-1px);
          box-shadow: 0 2px 6px rgba(0, 0, 0, 0.12), 0 6px 16px rgba(196, 146, 143, 0.3);
        }

        .login-form-submit-btn:active:not(:disabled) {
          transform: translateY(0);
        }

        .login-form-submit-btn:disabled {
          opacity: 0.7;
          cursor: not-allowed;
        }

        /* Spinner */
        .login-form-spinner {
          width: 20px;
          height: 20px;
          border: 2px solid rgba(255, 255, 255, 0.3);
          border-top-color: #FFFFFF;
          border-radius: 50%;
          animation: login-spin 0.6s linear infinite;
        }

        @keyframes login-spin {
          to { transform: rotate(360deg); }
        }

        /* Divider */
        .login-form-divider {
          display: flex;
          align-items: center;
          gap: 16px;
          margin: 28px 0;
        }

        .login-form-divider-line {
          flex: 1;
          height: 1px;
          background: #E5E7EB;
        }

        .login-form-divider-text {
          font-family: var(--font-poppins), sans-serif;
          font-size: 12px;
          font-weight: 500;
          color: #9CA3AF;
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }

        /* Social Buttons */
        .login-form-social {
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .login-form-social-btn {
          width: 100%;
          height: 48px;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 10px;
          border: 1.5px solid #E5E7EB;
          border-radius: 12px;
          background: #FFFFFF;
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          font-weight: 500;
          color: #374151;
          cursor: pointer;
          transition: all 0.15s;
        }

        .login-form-social-btn:hover:not(:disabled) {
          background: #F9FAFB;
          border-color: #D1D5DB;
        }

        .login-form-social-btn:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }

        /* Register link */
        .login-form-register {
          text-align: center;
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          color: #6B7280;
          margin: 28px 0 0;
        }

        .login-form-register-link {
          color: #6B9080;
          font-weight: 600;
          text-decoration: none;
          transition: color 0.15s;
        }

        .login-form-register-link:hover {
          color: #527063;
        }

        /* Responsive */
        @media (max-width: 640px) {
          .login-form-card {
            padding: 24px 20px;
            box-shadow: 0 8px 30px rgba(0, 0, 0, 0.04);
          }

          .login-form-title {
            font-size: 26px;
          }

          .login-form-subtitle {
            font-size: 14px;
          }

          .login-form-submit-btn {
            height: 48px;
            font-size: 15px;
          }
        }
      `}</style>
    </div>
  );
}
