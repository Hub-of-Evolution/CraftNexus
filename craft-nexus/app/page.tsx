import CategorySection from "@/components/landing/BrowseCategories";
import FeatureBar from "@/components/organisms/FeatureBar";
import FeaturedCourses from "@/components/organisms/FeaturedCourses";
import HeroSection from "@/components/organisms/HomepageHero";
import TestimonialsSection from "@/components/organisms/Testimonials";
import TopCourses from "@/components/organisms/TopCourses";

export default function Home() {
  return (
    <div className="">
      <HeroSection />
      <CategorySection />
      <TopCourses />
      <FeatureBar />
      <TestimonialsSection />
      <FeaturedCourses />
    </div>
  );
}
