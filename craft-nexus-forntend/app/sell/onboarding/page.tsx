"use client";

import OnboardingSlide from "@/components/sell/onboard/BoardingSlide";
import { ONBOARDING_SLIDES } from "@/components/sell/onboard/OnboardingData";
import { useRouter } from "next/navigation";

export default function SellOnboardingPage() {
  const router = useRouter();

  const handleGetStarted = () => {
    // Navigate to slide 2 when implemented; placeholder for now
    router.push("/sell/onboarding/2");
  };

  const handleSkip = () => {
    router.push("/sell/dashboard");
  };

  return (
    <OnboardingSlide
      slide={ONBOARDING_SLIDES[0]}
      currentIndex={0}
      totalSlides={ONBOARDING_SLIDES.length}
      onNext={handleGetStarted}
      onSkip={handleSkip}
    />
  );
}