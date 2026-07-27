"use client";

import Image from "next/image";
import { OnboardingSlideData } from "./OnboardingData";

interface OnboardingSlideProps {
  slide: OnboardingSlideData;
  currentIndex: number;
  totalSlides: number;
  onNext: () => void;
  onSkip: () => void;
}

export default function OnboardingSlide({
  slide,
  currentIndex,
  totalSlides,
  onNext,
  onSkip,
}: OnboardingSlideProps) {
  const isLastSlide = currentIndex === totalSlides - 1;

  return (
    <div className="onboarding-wrapper">
      {/* Badge */}
      {currentIndex === 2 &&
        <div className="onboarding-badge">
          <span className="badge-icon" aria-hidden="true">
            <svg width="18" height="14" viewBox="0 0 18 14" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M9 0L11.5 5.5L17.5 6.5L13 10.5L14.5 16.5L9 13.5L3.5 16.5L5 10.5L0.5 6.5L6.5 5.5L9 0Z" fill="#C4928F" />
            </svg>
          </span>
          <span className="badge-label">{slide.badge}</span>
        </div>
      }

      {/* Illustration */}
      <div className="onboarding-illustration">
        <Image
          src={slide.illustrationSrc}
          alt={slide.illustrationAlt}
          width={512}
          height={512}
          priority
          className="illustration-img"
        />
      </div>

      {/* Text */}
      <div className="onboarding-text">
        <h1 className="onboarding-title">
          {slide.title.split("\n").map((line, i) => (
            <span key={i}>
              {line}
              {i < slide.title.split("\n").length - 1 && <br />}
            </span>
          ))}
        </h1>
        <p className="onboarding-subtitle">
          {slide.subtitle.split("\n").map((line, i) => (
            <span key={i}>
              {line}
              {i < slide.subtitle.split("\n").length - 1 && <br />}
            </span>
          ))}
        </p>
      </div>

      {/* Progress Dots */}
      <div className="onboarding-dots" role="tablist" aria-label="Slide progress">
        {Array.from({ length: totalSlides }).map((_, i) => (
          <span
            key={i}
            role="tab"
            aria-selected={i === currentIndex}
            aria-label={`Slide ${i + 1} of ${totalSlides}`}
            className={`dot ${i === currentIndex ? "dot--active" : ""}`}
            style={i === currentIndex ? { width: '32px' } : {}}
          />
        ))}
      </div>

      {/* CTA */}
      <div className="onboarding-cta">
        <button
          className="btn-primary"
          onClick={onNext}
          type="button"
        >
          {slide.ctaLabel}
        </button>
        {isLastSlide ? (
          <button
            className="btn-secondary"
            onClick={onSkip} // Assuming skip or secondary action for "Already have account"
            type="button"
          >
            I Already Have an Account
          </button>
        ) : (
          <button
            className="btn-skip"
            onClick={onSkip}
            type="button"
          >
            Skip
          </button>
        )}
      </div>

      <style>{`
        .onboarding-wrapper {
          min-height: 100svh;
          display: flex;
          flex-direction: column;
          align-items: center;
          padding: 100px 24px 64px;
          background: #ffffff;
          gap: 0;
        }

        /* Badge */
        .onboarding-badge {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 8px 16px;
          background: rgba(196, 146, 143, 0.1);
          border-radius: 9999px;
          margin-bottom: 24px;
          order: 3; /* Move below text in original flex but design shows it above dots */
        }
        
        /* Re-ordering for Step 3 design */
        .onboarding-illustration { order: 1; }
        .onboarding-text { order: 2; }
        .onboarding-badge { order: 3; }
        .onboarding-dots { order: 4; }
        .onboarding-cta { order: 5; }

        .badge-icon {
          display: inline-flex;
          align-items: center;
          justify-content: center;
        }
        .badge-label {
          font-family: var(--font-poppins), sans-serif;
          font-size: 14px;
          font-weight: 500;
          color: #374151;
          letter-spacing: -0.5px;
        }

        /* Illustration */
        .onboarding-illustration {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 100%;
          margin-bottom: 48px;
        }
        .illustration-img {
          width: 100%;
          max-width: 512px;
          height: auto;
          object-fit: contain;
        }

        /* Text */
        .onboarding-text {
          text-align: center;
          margin-bottom: 24px;
        }
        .onboarding-title {
          font-family: var(--font-poppins), sans-serif;
          font-size: 48px;
          font-weight: 700;
          color: #111827;
          line-height: 48px;
          margin: 0 0 16px;
          letter-spacing: -0.5px;
        }
        .onboarding-subtitle {
          font-family: var(--font-poppins), sans-serif;
          font-size: 18px;
          color: #4B5563;
          line-height: 28px;
          margin: 0 auto;
          max-width: 520px;
          letter-spacing: -0.5px;
        }

        /* Dots */
        .onboarding-dots {
          display: flex;
          gap: 8px;
          margin-bottom: 32px;
        }
        .dot {
          display: block;
          width: 8px;
          height: 8px;
          border-radius: 9999px;
          background: #D9D9D9;
          transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
        }
        .dot--active {
          background: #C4928F;
        }

        /* CTA */
        .onboarding-cta {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 12px;
          width: 100%;
          max-width: 624px;
        }
        .btn-primary {
          width: 352px;
          height: 60px;
          background: #C4928F;
          color: #ffffff;
          border: none;
          border-radius: 16px;
          font-family: var(--font-outfit), sans-serif;
          font-size: 18px;
          font-weight: 400;
          cursor: pointer;
          transition: all 0.2s;
          letter-spacing: -0.5px;
          box-shadow: 0px 4px 6px rgba(0, 0, 0, 0.1), 0px 10px 15px rgba(0, 0, 0, 0.1);
          display: flex;
          align-items: center;
          justify-content: center;
        }
        .btn-primary:hover {
          background: #b07c76;
          transform: translateY(-1px);
          box-shadow: 0px 6px 8px rgba(0, 0, 0, 0.15), 0px 12px 20px rgba(0, 0, 0, 0.15);
        }
        .btn-primary:active {
          transform: translateY(0);
        }
        .btn-skip, .btn-secondary {
          background: none;
          border: none;
          color: #4B5563;
          font-family: var(--font-poppins), sans-serif;
          font-size: 16px;
          font-weight: 500;
          cursor: pointer;
          padding: 12px;
          transition: color 0.15s;
          letter-spacing: -0.5px;
        }
        .btn-skip:hover, .btn-secondary:hover {
          color: #111827;
        }

        /* Responsive */
        @media (max-width: 768px) {
          .onboarding-wrapper {
            padding: 60px 24px;
          }
          .onboarding-title {
            font-size: 32px;
            line-height: 36px;
          }
          .onboarding-subtitle {
            font-size: 16px;
            line-height: 24px;
          }
          .btn-primary {
            width: 100%;
          }
          .illustration-img {
            max-width: 320px;
          }
        }
      `}</style>
    </div>
  );
}