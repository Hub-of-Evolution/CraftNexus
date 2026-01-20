import Link from "next/link";
import Image from "next/image";
import { Facebook, Instagram } from "../icon";

// Reusable style constants
const sectionTitleClass =
  "font-ibm-plex-serif text-white font-semibold leading-normal text-[20px] text-nowrap";
const linkClass =
  " text-sm md:text-[16px] font-poppins hover:text-gray-200 text-white font-normal leading-normal text-nowrap tracking-[1.08px]";

const navigation = {
  quickLinks: [
    { name: "Online Courses", href: "/courses/online" },
    { name: "On-site Courses", href: "/courses/onsite" },
    { name: "Entrepreneurship", href: "/entrepreneurship" },
  ],
  community: [
    { name: "Blog", href: "/blog" },
    { name: "Alumni", href: "/alumni" },
    { name: "Testimonials", href: "/testimonials" },
    { name: "Instructors", href: "/instructors" },
    { name: "Customer Support", href: "/support" },
  ],
  aboutUs: [
    { name: "About Craft Nexus", href: "/about" },
    { name: "Terms and Conditions", href: "/terms" },
    { name: "Refund Policy", href: "/refund-policy" },
  ],
  social: [
    {
      name: "Facebook",
      href: "https://facebook.com/craftnexus",
      icon: Facebook,
    },
    {
      name: "Instagram",
      href: "https://instagram.com/craftnexus",
      icon: Instagram,
    },
  ],
};

export default function Footer() {
  return (
    <footer className="bg-[#517a77] text-white">
      <div className="mx-auto max-w-7xl px-6 pt-10 lg:pt-20 pb-16 ">
        <div className="grid gap-8 lg:gap-10 xl:gap-14 grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-[200px_repeat(4,1fr)] xl:grid-cols-[260px_repeat(4,1fr)]">
          {/* logo column */}
          <div className="flex flex-col justify-center items-center w-48 h-48 lg:w-50 lg:h-50 xl:w-64 xl:h-64">
            <Link href="/">
              <Image
                src="/footerlogo.svg"
                alt="Craft Nexus Logo"
                width={258}
                height={258}
                className="h-full w-full aspect-square justify-baseline object-cover"
              />
            </Link>
          </div>

          {/* Quick Links Column */}
          <div className="items-start flex justify-start flex-col xl:pt-4">
            <h3 className={sectionTitleClass}>Quick Links</h3>
            <ul role="list" className="lg:mt-6 mt-2 space-y-4">
              {navigation.quickLinks.map((item) => (
                <li key={item.name}>
                  <Link href={item.href} className={linkClass}>
                    {item.name}
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          {/* Community Column */}
          <div className="items-start flex justify-start flex-col xl:pt-4">
            <h3 className={sectionTitleClass}>Community</h3>
            <ul role="list" className="lg:mt-6 mt-2 space-y-4">
              {navigation.community.map((item) => (
                <li key={item.name}>
                  <Link href={item.href} className={linkClass}>
                    {item.name}
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          {/* About Us Column */}
          <div className="items-start flex justify-start flex-col xl:pt-4">
            <h3 className={sectionTitleClass}>About Us</h3>
            <ul role="list" className="lg:mt-6 mt-2 space-y-4">
              {navigation.aboutUs.map((item) => (
                <li key={item.name}>
                  <Link href={item.href} className={linkClass}>
                    {item.name}
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          {/* Social Links Column */}
          <div className="items-start flex justify-start flex-col xl:pt-4">
            <h3 className={sectionTitleClass}>Social Links</h3>
            <div className="lg:mt-6 mt-2 flex gap-x-4">
              {navigation.social.map((item) => {
                const Icon = item.icon;
                return (
                  <Link
                    key={item.name}
                    href={item.href}
                    className={linkClass}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    <span className="sr-only">{item.name}</span>
                    <Icon className="h-6 w-6" />
                  </Link>
                );
              })}
            </div>
          </div>
        </div>

        {/* Copyright - unchanged */}
        <div className="lg:mt-16 pt-8 mt-10">
          <p className="text-white text-center md:text-left font-poppins text-[16px] font-normal leading-normal tracking-[1.08px]">
            @{new Date().getFullYear()} | Craft Nexus. All rights reserved.
          </p>
        </div>
      </div>
    </footer>
  );
}
