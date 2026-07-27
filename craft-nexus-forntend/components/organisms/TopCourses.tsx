import Image from "next/image";
import { BsHandThumbsUp } from "react-icons/bs";
import { CiUser } from "react-icons/ci";
import { IoLocationOutline } from "react-icons/io5";
import { LuEllipsis } from "react-icons/lu";

export default function TopCourses() {
  return (
    <section className="max-sm:px-4">
      <h2 className="sm:ml-[72px] text-[#272727] text-[50px] font-bold font-ibm-plex-serif">
        Top Courses
      </h2>
      <div className="grid lg:grid-cols-3  md:grid-cols-2 grid-cols-1 gap-7 max-w-[1427px] mx-auto mt-[40px] justify-items-center">
        {data.map((course, index) => (
          <CourseCard key={index} {...course} />
        ))}
      </div>
      <div className="flex items-center gap-3 justify-center mt-10.5 mb-14">
        <ButtonPagination active={true}>1</ButtonPagination>
        <ButtonPagination>2</ButtonPagination>
        <ButtonPagination>3</ButtonPagination>
        <LuEllipsis />
        <ButtonPagination>67</ButtonPagination>
        <ButtonPagination>68</ButtonPagination>
      </div>
    </section>
  );
}

function CourseCard({
  title,
  by,
  location,
  description,
  students,
  rating,
  price,
  image,
  hours,
}: {
  title: string;
  by: string;
  location: string;
  description: string;
  students: number;
  rating: string;
  price: string;
  image: string;
  hours: string;
}) {
  return (
    <div className="font-inter rounded-[20px] bg-[#D6B3B11A] max-w-[454px] text-black overflow-hidden">
      <div className="relative">
        <div className="absolute right-[22px] top-5 flex justify-center items-center w-[92px] h-[38px] bg-white rounded-[5px] text-black">
          {hours} Hours
        </div>
        <Image
          src={image}
          alt="Course Image"
          width={454}
          height={337}
          className="object-cover"
        />
      </div>
      <div className="px-5 pt-7 text-[22px]/6 font-extralight ">
        <h3 className="font-bold ">{title}</h3>
        <p className="italic pt-2">
          Art by <span className="font-normal">{by}</span>
        </p>
        <p className="text-[18px] font-thin flex items-center gap-1.5 pt-2">
          <IoLocationOutline className="" />
          {location}
        </p>
        <p className="pt-6">{description}</p>
        <div className="flex gap-3 pt-5">
          <span className="flex items-center gap-1.5">
            <CiUser />
            {students}
          </span>
          <span className="flex items-center gap-1.5">
            <BsHandThumbsUp />
            {rating}
          </span>
        </div>
      </div>
      <button className="mt-[42px] font-bold flex justify-center items-center w-full bg-[#B1D4D6] h-[61px] cursor-pointer hover:bg-[#B1D4D6]/80 transition-colors duration-300">
        Buy for {price}
      </button>
    </div>
  );
}

function ButtonPagination({
  children,
  active,
}: {
  children: React.ReactNode;
  active?: boolean;
}) {
  return (
    <button
      className={`${active ? "bg-[#2C2C2C] text-white" : "bg-[#fff] text-black"} hover:bg-[#2C2C2C] hover:text-white transition-all w-[32px] h-[32px] rounded-[8px] flex justify-center items-center`}
    >
      {children}
    </button>
  );
}
const data = [
  {
    title: "Color and Lighting Techniques with Acrylic Paint",
    by: "Paintyourlife",
    location: "5 Vazha Pshavela Street, Tbilisi, Georgia",
    description:
      "Learn to work with color harmoniously to create the perfect illustrations",
    students: 230,
    rating: "96%",
    price: "0.9 USDC",
    image: "/bg.jpg",
    hours: "40",
  },
  {
    title: "Color and Lighting Techniques with Acrylic Paint",
    by: "Paintyourlife",
    location: "5 Vazha Pshavela Street, Tbilisi, Georgia",
    description:
      "Learn to work with color harmoniously to create the perfect illustrations",
    students: 530,
    rating: "98%",
    price: "0.9 USDC",
    image: "/bg2.jpg",
    hours: "20",
  },
  {
    title: "Color and Lighting Techniques with Acrylic Paint",
    by: "Paintyourlife",
    location: "5 Vazha Pshavela Street, Tbilisi, Georgia",
    description:
      "Learn to work with color harmoniously to create the perfect illustrations",
    students: 230,
    rating: "96%",
    price: "0.9 USDC",
    image: "/bg3.jpg",
    hours: "40",
  },
  {
    title: "Color and Lighting Techniques with Acrylic Paint",
    by: "Paintyourlife",
    location: "5 Vazha Pshavela Street, Tbilisi, Georgia",
    description:
      "Learn to work with color harmoniously to create the perfect illustrations",
    students: 130,
    rating: "96%",
    price: "0.9 USDC",
    image: "/bg.jpg",
    hours: "27",
  },
  {
    title: "Color and Lighting Techniques with Acrylic Paint",
    by: "Paintyourlife",
    location: "5 Vazha Pshavela Street, Tbilisi, Georgia",
    description:
      "Learn to work with color harmoniously to create the perfect illustrations",
    students: 830,
    rating: "95%",
    price: "0.9 USDC",
    image: "/bg3.jpg",
    hours: "40",
  },
  {
    title: "Color and Lighting Techniques with Acrylic Paint",
    by: "Paintyourlife",
    location: "5 Vazha Pshavela Street, Tbilisi, Georgia",
    description:
      "Learn to work with color harmoniously to create the perfect illustrations",
    students: 230,
    rating: "99%",
    price: "0.9 USDC",
    image: "/bg2.jpg",
    hours: "40",
  },
];
