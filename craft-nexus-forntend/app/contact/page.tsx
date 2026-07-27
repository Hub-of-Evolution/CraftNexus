"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

interface ContactFormData {
  firstName: string;
  lastName: string;
  email: string;
  subject: string;
  message: string;
}

interface FormErrors {
  firstName?: string;
  lastName?: string;
  email?: string;
  subject?: string;
  message?: string;
}

const SUBJECT_OPTIONS = [
  "General Inquiry",
  "Product Support",
  "Order Issue",
  "Partnership",
  "Feedback",
  "Other",
];

function validateEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
}

export default function ContactPage() {
  const router = useRouter();

  const [formData, setFormData] = useState<ContactFormData>({
    firstName: "",
    lastName: "",
    email: "",
    subject: "General Inquiry",
    message: "",
  });

  const [errors, setErrors] = useState<FormErrors>({});
  const [isSubmitting, setIsSubmitting] = useState(false);

  const validate = (): FormErrors => {
    const newErrors: FormErrors = {};
    if (!formData.firstName.trim())
      newErrors.firstName = "First name is required.";
    if (!formData.lastName.trim())
      newErrors.lastName = "Last name is required.";
    if (!formData.email.trim()) {
      newErrors.email = "Email address is required.";
    } else if (!validateEmail(formData.email)) {
      newErrors.email = "Please enter a valid email address.";
    }
    if (!formData.subject) newErrors.subject = "Please select a subject.";
    if (!formData.message.trim())
      newErrors.message = "Message is required.";
    return newErrors;
  };

  const handleChange = (
    e: React.ChangeEvent<
      HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement
    >
  ) => {
    const { name, value } = e.target;
    setFormData((prev) => ({ ...prev, [name]: value }));
    // Clear field error on change
    if (errors[name as keyof FormErrors]) {
      setErrors((prev) => ({ ...prev, [name]: undefined }));
    }
  };

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const validationErrors = validate();
    if (Object.keys(validationErrors).length > 0) {
      setErrors(validationErrors);
      return;
    }
    setIsSubmitting(true);
    // Simulate async submission
    await new Promise((res) => setTimeout(res, 600));
    setIsSubmitting(false);
    router.push("/signup");
  };

  return (
    <div className="min-h-screen bg-[#F9FAFB] px-4 sm:px-6 md:px-10 lg:px-20 items-center justify-center grid grid-cols-1 md:grid-cols-3 p-4">
      {/* Card */}
      <div
        className="w-full bg-[#F9FAFB] rounded-2xl p-8 md:p-10"
        style={{
          maxWidth: "576px",
          boxShadow:
            "0px 4px 6px rgba(0,0,0,0.1), 0px 10px 15px rgba(0,0,0,0.1)",
          borderRadius: "16px",
        }}
      >
        {/* Heading */}
        <h1 className="text-2xl font-bold text-[#1A1A1A] mb-7">
          Send us a Message
        </h1>

        <form onSubmit={handleSubmit} noValidate className="space-y-5">
          {/* First Name / Last Name row */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <FormField label="First Name" error={errors.firstName}>
              <input
                id="firstName"
                name="firstName"
                type="text"
                placeholder="John"
                value={formData.firstName}
                onChange={handleChange}
                className={inputCls(!!errors.firstName)}
              />
            </FormField>

            <FormField label="Last Name" error={errors.lastName}>
              <input
                id="lastName"
                name="lastName"
                type="text"
                placeholder="Doe"
                value={formData.lastName}
                onChange={handleChange}
                className={inputCls(!!errors.lastName)}
              />
            </FormField>
          </div>

          {/* Email Address */}
          <FormField label="Email Address" error={errors.email}>
            <input
              id="email"
              name="email"
              type="email"
              placeholder="john@example.com"
              value={formData.email}
              onChange={handleChange}
              className={inputCls(!!errors.email)}
            />
          </FormField>

          {/* Subject */}
          <FormField label="Subject" error={errors.subject}>
            <div className="relative">
              <select
                id="subject"
                name="subject"
                value={formData.subject}
                onChange={handleChange}
                className={selectCls(!!errors.subject)}
              >
                {SUBJECT_OPTIONS.map((opt) => (
                  <option key={opt} value={opt}>
                    {opt}
                  </option>
                ))}
              </select>
              {/* Custom chevron */}
              <div className="pointer-events-none absolute inset-y-0 right-3 flex items-center">
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 20 20"
                  fill="none"
                  xmlns="http://www.w3.org/2000/svg"
                  aria-hidden="true"
                >
                  <path
                    d="M5 7.5L10 12.5L15 7.5"
                    stroke="#6B7280"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              </div>
            </div>
          </FormField>

          {/* Message */}
          <FormField label="Message" error={errors.message}>
            <textarea
              id="message"
              name="message"
              placeholder="Tell us how we can help you..."
              rows={5}
              value={formData.message}
              onChange={handleChange}
              className={`${inputCls(!!errors.message)} resize-none`}
            />
          </FormField>

          {/* Submit button */}
          <button
            type="submit"
            disabled={isSubmitting}
            className="w-full bg-[#BC8B83] hover:bg-[#a87970] active:scale-[0.98] disabled:opacity-60 disabled:cursor-not-allowed transition-all duration-200 text-white font-semibold text-sm py-3.5 rounded-xl mt-2 shadow-md shadow-[#BC8B83]/30"
          >
            {isSubmitting ? "Sending…" : "Send Message"}
          </button>
        </form>
      </div>
    </div>
  );
}

/* ── Helpers ─────────────────────────────────────────────────── */

function FormField({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label className="text-[13px] font-medium text-[#374151]">{label}</label>
      {children}
      {error && (
        <span className="text-[11px] text-red-500 leading-tight">{error}</span>
      )}
    </div>
  );
}

function inputCls(hasError: boolean): string {
  return [
    "w-full rounded-lg border px-3.5 py-2.5 text-sm text-[#1A1A1A]",
    "placeholder:text-[#9CA3AF] bg-white outline-none transition-all duration-150",
    "focus:ring-2 focus:ring-[#C48C8C] focus:border-transparent",
    hasError ? "border-red-400" : "border-[#D1D5DB]",
  ].join(" ");
}

function selectCls(hasError: boolean): string {
  return [
    inputCls(hasError),
    "appearance-none pr-9 cursor-pointer",
  ].join(" ");
}
