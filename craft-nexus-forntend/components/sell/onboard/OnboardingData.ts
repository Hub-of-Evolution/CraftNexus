export interface OnboardingSlideData {
  badge: string;
  illustrationAlt: string;
  illustrationSrc: string;
  title: string;
  subtitle: string;
  ctaLabel: string;
}

export const ONBOARDING_SLIDES: OnboardingSlideData[] = [
  {
    badge: "Sell & Learn",
    illustrationAlt: "Artisan crafting pottery",
    illustrationSrc: "/illustration/pottery-artisan.svg",
    title: "Turn Your Craft\nInto Income",
    subtitle: "Sell handmade products and learn\ncreative skills from experts.",
    ctaLabel: "Get Started",
  },
  // Slide 2 placeholder — fill in when implementing issue #32
  {
    badge: "10K+ Creators Joined",
    illustrationAlt: "Artisan showcasing products",
    illustrationSrc: "/illustration/artisan-showcase.svg",
    title: "Learn From\nSkilled Creators",
    subtitle: "Access video tutorials and step-by-step guides to master your craft at your own pace.",
    ctaLabel: "Next",
  },
  // Slide 3 - Artisans Complete
  {
    badge: "10K+ Creators Joined",
    illustrationAlt: "Artisan earning income",
    illustrationSrc: "/artisan-earning.svg",
    title: "Sell Your Craft.\nEarn More.",
    subtitle: "Create your shop, reach customers worldwide, and turn your passion into income.",
    ctaLabel: "Next",
  },
];