export interface CourseCardProps {
  title: string;
  author: string;
  description: string;
  students: number;
  rating: number;
  duration: string;
  price: string;
  image?: string;
}

export default function CourseCard({
  title,
  author,
  description,
  students,
  rating,
  duration,
  price,
}: CourseCardProps) {
  return (
    <div className="bg-white rounded-lg overflow-hidden border border-gray-200 hover:shadow-md transition-shadow duration-200">
      {/* Image Section */}
      <div className="relative h-40 bg-gray-200">
        <div className="absolute top-2 right-2 bg-white/90 px-2 py-1 rounded-md text-xs font-medium text-gray-700">
          {duration}
        </div>
      </div>

      {/* Content */}
      <div className="p-4">
        <h3 className="font-semibold text-gray-800 mb-1 line-clamp-2">{title}</h3>
        <p className="text-sm text-gray-500 mb-2">{author}</p>
        <p className="text-sm text-gray-600 line-clamp-2 mb-3">{description}</p>

        {/* Stats Row */}
        <div className="flex items-center justify-between mb-3">
          <span className="text-sm text-gray-500">{students} students</span>
          <div className="flex items-center gap-1">
            <span className="text-sm text-gray-500">{rating}</span>
            <span className="text-yellow-500">★</span>
          </div>
        </div>

        {/* CTA Button */}
        <button className="w-full py-2 px-4 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors duration-200 font-medium">
          {price}
        </button>
      </div>
    </div>
  );
}
