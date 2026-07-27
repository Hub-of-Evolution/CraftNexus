"use client";

import React from "react";
import { LuGraduationCap, LuStar, LuPuzzle } from "react-icons/lu";

interface FeatureItemProps {
  icon: React.ReactNode;
  title: string;
  description: string;
}

const FeatureItem = ({ icon, title, description }: FeatureItemProps) => (
  <div className="flex items-center gap-4 md:gap-5 flex-1 min-w-[280px]">
    <div className="bg-white p-3 sm:p-4 rounded-xl shadow-[0_10px_15px_-3px_rgba(0,0,0,0.1),0_4px_6px_-4px_rgba(0,0,0,0.1)] flex items-center justify-center shrink-0">
      <div className="text-[#C4928F] text-2xl sm:text-3xl md:text-4xl">
        {icon}
      </div>
    </div>
    <div className="text-white space-y-1">
      <h3 className="font-serif text-lg sm:text-xl md:text-sm font-bold leading-tight">
        {title}
      </h3>
      <p className="text-xs  opacity-90 leading-relaxed font-sans">
        {description}
      </p>
    </div>
  </div>
);

export default function FeatureBar() {
  const features = [
    {
      icon: <LuGraduationCap />,
      title: "Best Course Providers",
      description: "We monitor and choose the instructors with best skills",
    },
    {
      icon: <LuStar />,
      title: "High Quality Video Courses",
      description: "High Quality Online courses are crafted with help of our production team",
    },
    {
      icon: <LuPuzzle />,
      title: "Preserving Traditional Handcraft",
      description: "students learn skills and techniques from experienced artisans, ensuring the continuity of tradition",
    },
  ];

  return (
    <section className="w-full bg-[#C4928F] py-10 md:py-14">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 md:px-10">
        <div className="flex flex-col lg:flex-row gap-8 lg:gap-12 items-start lg:items-center justify-between">
          {features.map((feature, index) => (
            <FeatureItem
              key={index}
              icon={feature.icon}
              title={feature.title}
              description={feature.description}
            />
          ))}
        </div>
      </div>
    </section>
  );
}
