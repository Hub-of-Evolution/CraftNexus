export default function Sidebar() {
  const courses = [
    { label: "All Courses", active: false },
    { label: "Online Courses", active: true },
    { label: "On-Site Courses", active: false },
    { label: "Entrepreneurship Courses", active: false },
    { label: "New Courses", active: false },
    { label: "Top Rated Courses", active: false },
  ];

  const categories = [
    { label: "Ceramics", active: false },
    { label: "Tufting", active: false },
    { label: "Crocheting", active: false },
    { label: "Sewing", active: false },
    { label: "Painting", active: false },
    { label: "Drawing", active: false },
  ];

  const filters = [
    { label: "Duration" },
    { label: "Type" },
    { label: "Provider" },
    { label: "Price" },
  ];

  return (
    <aside className="w-full md:w-64 bg-white p-4">
      {/* Courses Section */}
      <div className="mb-6">
        <h3 className="font-semibold text-gray-800 mb-3">Courses</h3>
        <nav className="space-y-1">
          {courses.map((item, index) => (
            <button
              key={index}
              className={`w-full text-left px-3 py-2 rounded-lg text-sm transition-colors duration-200 ${
                item.active
                  ? "bg-blue-100 text-blue-700 font-medium"
                  : "text-gray-600 hover:bg-gray-50"
              }`}
            >
              {item.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Categories Section */}
      <div className="mb-6">
        <h3 className="font-semibold text-gray-800 mb-3">Categories</h3>
        <nav className="space-y-1">
          {categories.map((item, index) => (
            <button
              key={index}
              className="w-full text-left px-3 py-2 rounded-lg text-sm text-gray-600 hover:bg-gray-50 transition-colors duration-200"
            >
              {item.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Filters Section */}
      <div>
        <h3 className="font-semibold text-gray-800 mb-3">Filters</h3>
        <nav className="space-y-1">
          {filters.map((item, index) => (
            <button
              key={index}
              className="w-full text-left px-3 py-2 rounded-lg text-sm text-gray-600 hover:bg-gray-50 transition-colors duration-200"
            >
              {item.label}
            </button>
          ))}
        </nav>
      </div>
    </aside>
  );
}
