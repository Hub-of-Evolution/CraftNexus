"use client";

import { useState } from "react";
import { FaStar } from "react-icons/fa";
import {
  CeramicLogo,
  TuftingLogo,
  SculptLogo,
  CrochetLogo,
} from "../molecules/FeaturedCourseLogos";
const categories = ["Online Courses", "On-site Courses", "Entrepreneurship Courses"];

const courses = [
  {
    id: 1,
    logo: CeramicLogo,
    logoLabel: "allumeya",
    title: "Creating Your First Ceramic Vessel",
    description: "Learn the entire process of working with clay at home, from the initial design to the final glaze.",
    students: 402,
    rating: 5,
    bg: "bg-[#c8e6d8]",
  },
  {
    id: 2,
    logo: TuftingLogo,
    logoLabel: "Tufte",
    title: "Tufting Technique for Creating Rugs",
    description: "Learn to master the tufting gun to create a striking rug filled with shapes, colors and textures.",
    students: 540,
    rating: 5,
    bg: "bg-[#c8e6d8]",
  },
  {
    id: 3,
    logo: SculptLogo,
    logoLabel: "Sculpt",
    title: "Cardboard Sculptures for Beginners",
    description: "Learn to transform your favorite subjects into cardboard sculptures of any size, playing with shape.",
    students: 145,
    rating: 5,
    bg: "bg-[#c8e6d8]",
  },
  {
    id: 4,
    logo: CrochetLogo,
    logoLabel: "Korve",
    title: "Creating Garments Using Crochet",
    description: "Learn to create colorful and textured garments using a crochet hook.",
    students: 230,
    rating: 5,
    bg: "bg-[#c8e6d8]",
  },
];

function StarRating({ rating }: { rating: number }) {
  return (
    <div className="flex gap-0.5">
      {[1, 2, 3, 4, 5].map((star) => (
        <FaStar
          key={star}
          className={star <= rating ? "text-[#C4928F]" : "text-[#C4928F]"}
          size={14}
        />
      ))}
    </div>
  );
}

function CourseCard({ course }: { course: (typeof courses)[0] }) {
      const Logo = course.logo;

  return (
    <div
      className={`${course.bg} rounded-[20px] p-4 flex flex-col gap-4 shadow-sm bg-[#B5E5CF80]`}
      style={{ minHeight: "220px", boxShadow: "20px 20px 4px 0px #00000040", }}
    >
      {/* Top row: logo + rating */}
      <div className="flex items-start justify-between">
        <div className="flex items-center gap-2">
          <div className="w-8 h-8  flex items-center justify-center ">
            <Logo className="w-5 h-5" />
          </div>
          <span className="text-xs font-semibold text-[#3d6b56]">{course.logoLabel}</span>
        </div>
        <StarRating rating={course.rating} />
      </div>

      {/* Title */}
      <h3 className="text-[16px] font-bold font-600 text-[#272727] leading-snug">{course.title}</h3>

      {/* Description */}
      <p className="text-xs text-[#272727] font-300 font-light justify max-w-[300px] leading-relaxed flex-1">{course.description}</p>

      {/* Footer */}
      <div className="flex items-center justify-between ">
        <span className="text-xs text-[#272727]">{course.students} Students Enrolled</span>
        <button className=" text-[#3D5B59]  border border-[#3D5B59] text-xs font-semibold px-4 py-1.5 rounded-[10px] shadow-sm hover:bg-[#2d6a52] hover:text-white transition-colors duration-200">
          Enroll Now
        </button>
      </div>
    </div>
  );
}

export default function FeaturedCourses() {
  const [activeTab, setActiveTab] = useState("Online Courses");
 
  return (
    <section className="py-14 px-4 bg-white">
      <div className="max-w-4xl mx-auto">
        {/* Title */}
        <h2 className="font-serif text-[2rem] sm:text-[2.15rem] md:text-[2.5rem] font-bold text-[#272727] mb-6 md:mb-6 text-center tracking-tight">
          Featured Courses
        </h2>

        {/* Tabs */}
        <div className="flex justify-center gap-2 mb-8 flex-wrap">
          {categories.map((cat) => (
            <button
              key={cat}
              onClick={() => setActiveTab(cat)}
            className={`px-6 py-2.5 rounded-[10px] text-sm font-medium transition-colors duration-200 border border-[#3D5B59] [box-shadow:10px_10px_4px_0px_rgba(0,0,0,0.25)] ${
  activeTab === cat
    ? "bg-[#517A77] text-white"
    : "bg-[#D6B3B1] text-[#FFFFFF] hover:bg-[#d9c4bc]"
}`}


               >
              {cat}
            </button>
          ))}
        </div>

        {/* Grid */}
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-2 gap-5">
          {courses.map((course) => (
            <CourseCard key={course.id} course={course} />
          ))}
        </div>

        {/* CTA */}
        <div className="flex justify-center mt-10">
          <button 
        className="mt-10 px-11 py-3.5 rounded-[10px] bg-[#517A77] text-white text-sm font-semibold tracking-wide hover:bg-[#2e5350] hover:-translate-y-0.5 active:translate-y-0 transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#3d6b68] focus-visible:ring-offset-2 shadow-[0_20px_20px_rgba(100,40,40,0.22)] "
          >
            Join Workshop
          </button>
        </div>
      </div>
    </section>
  );
}