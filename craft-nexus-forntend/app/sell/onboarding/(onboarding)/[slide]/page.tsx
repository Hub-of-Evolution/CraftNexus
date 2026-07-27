"use client";

import OnboardingSlide from "@/components/sell/onboard/BoardingSlide";
import { ONBOARDING_SLIDES } from "@/components/sell/onboard/OnboardingData";
import { useRouter, useParams, notFound } from "next/navigation";

/**
 * Sell Onboarding Slide Page - Dynamic route for subsequent slides
 * Handles navigation between onboarding slides
 */
export default function SellOnboardingSlidePage() {
  const router = useRouter();
  const params = useParams();
  const slideNumber = Number(params?.slide);

  // slideNumber is 1-based in the URL; index is 0-based
  const slideIndex = slideNumber - 1;

  // Validate slide number
  if (
    isNaN(slideNumber) ||
    slideIndex < 0 ||
    slideIndex >= ONBOARDING_SLIDES.length
  ) {
    notFound();
  }

  const isLastSlide = slideIndex === ONBOARDING_SLIDES.length - 1;

  const handleNext = () => {
    if (isLastSlide) {
      router.push("/sell/dashboard");
    } else {
      router.push(`/sell/onboarding/${slideNumber + 1}`);
    }
  };

  const handleSkip = () => {
    router.push("/sell/dashboard");
  };

  return (
    <OnboardingSlide
      slide={ONBOARDING_SLIDES[slideIndex]}
      currentIndex={slideIndex}
      totalSlides={ONBOARDING_SLIDES.length}
      onNext={handleNext}
      onSkip={handleSkip}
    />
  );
}
