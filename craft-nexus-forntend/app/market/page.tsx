"use client";
import { useState } from "react";
import { FaBars, FaTimes } from "react-icons/fa";
import Sidebar from "@/components/features/market/Sidebar";
import CourseCard from "@/components/features/market/CourseCard";

interface Course {
  title: string;
  author: string;
  description: string;
  students: number;
  rating: number;
  duration: string;
  price: string;
}

const courses: Course[] = [
  {
    title: "Introduction to Pottery",
    author: "Sarah Mitchell",
    description: "Learn the fundamentals of wheel throwing and hand-building techniques.",
    students: 250,
    rating: 4.8,
    duration: "40 Hours",
    price: "Enroll for 2.99 USDC",
  },
  {
    title: "Modern Tufting Masterclass",
    author: "James Cooper",
    description: "Create stunning tufted rugs and wall art with professional techniques.",
    students: 180,
    rating: 4.9,
    duration: "25 Hours",
    price: "Enroll for 3.99 USDC",
  },
  {
    title: "Crochet for Beginners",
    author: "Emily Watson",
    description: "Start your crocheting journey with basic stitches and patterns.",
    students: 420,
    rating: 4.7,
    duration: "15 Hours",
    price: "Enroll for 1.99 USDC",
  },
  {
    title: "Advanced Sewing Techniques",
    author: "Michael Brooks",
    description: "Master complex sewing projects with professional finishing touches.",
    students: 165,
    rating: 4.6,
    duration: "35 Hours",
    price: "Enroll for 4.99 USDC",
  },
  {
    title: "Watercolor Painting Basics",
    author: "Lisa Anderson",
    description: "Discover beautiful watercolor techniques for stunning artwork.",
    students: 310,
    rating: 4.8,
    duration: "20 Hours",
    price: "Enroll for 2.49 USDC",
  },
  {
    title: " charcoal Drawing Fundamentals",
    author: "David Kim",
    description: "Build strong drawing skills with traditional charcoal techniques.",
    students: 195,
    rating: 4.5,
    duration: "18 Hours",
    price: "Enroll for 1.99 USDC",
  },
];

export default function MarketPage() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);

  const toggleSidebar = () => {
    setIsSidebarOpen(!isSidebarOpen);
  };

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Mobile Sidebar Toggle */}
      <div className="md:hidden fixed top-4 left-4 z-50">
        <button
          onClick={toggleSidebar}
          className="bg-white p-2 rounded-lg shadow-md"
        >
          {isSidebarOpen ? (
            <FaTimes className="text-gray-800 text-xl" />
          ) : (
            <FaBars className="text-gray-800 text-xl" />
          )}
        </button>
      </div>

      {/* Main Content */}
      <div className="flex flex-col md:flex-row">
        {/* Sidebar */}
        <div className={`
          fixed md:relative top-0 left-0 h-full w-64 bg-white shadow-lg transform transition-transform duration-300 ease-in-out z-40
          ${isSidebarOpen ? 'translate-x-0' : '-translate-x-full'}
          md:translate-x-0 md:shadow-none md:h-auto
        `}>
          <div className="pt-16 md:pt-0">
            <Sidebar />
          </div>
        </div>

        {/* Overlay for mobile */}
        {isSidebarOpen && (
          <div
            className="fixed inset-0 bg-black bg-opacity-50 z-30 md:hidden"
            onClick={() => setIsSidebarOpen(false)}
          />
        )}

        {/* Course Grid Area */}
        <div className="flex-1 p-4 md:p-6 pt-16 md:pt-6">
          {/* Top Section */}
          <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center mb-6 gap-4">
            <h1 className="text-2xl md:text-3xl font-bold text-gray-800">
              Online Handcraft Courses
            </h1>
            <select className="px-4 py-2 border border-gray-300 rounded-lg bg-white text-gray-700 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500">
              <option>Sort by Popular</option>
              <option>Sort by Newest</option>
              <option>Sort by Price: Low to High</option>
              <option>Sort by Price: High to Low</option>
              <option>Sort by Rating</option>
            </select>
          </div>

          {/* Course Grid */}
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {courses.map((course, index) => (
              <CourseCard
                key={index}
                title={course.title}
                author={course.author}
                description={course.description}
                students={course.students}
                rating={course.rating}
                duration={course.duration}
                price={course.price}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
