import * as React from 'react';
import { cn } from '@/lib/utils';

/**
 * Custom vector iconography and 3D-styled illustrations matching the
 * reference desktop recovery suite chrome, sidebar, and landing cards.
 */

export function BrandBriefcaseLogo({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 36 36"
      fill="none"
      className={cn('size-6 shrink-0', className)}
      aria-hidden
    >
      <defs>
        <linearGradient
          id="brand-bg"
          x1="0"
          y1="0"
          x2="36"
          y2="36"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0%" stopColor="#3B9BFF" />
          <stop offset="100%" stopColor="#1677FF" />
        </linearGradient>
      </defs>
      <path
        d="M13 6.5C13 5.11929 14.1193 4 15.5 4H20.5C21.8807 4 23 5.11929 23 6.5V8H13V6.5Z"
        fill="#0958D9"
      />
      <rect x="3" y="7" width="30" height="25" rx="6" fill="url(#brand-bg)" />
      <rect x="3" y="7" width="30" height="11" rx="6" fill="#69B1FF" fillOpacity="0.28" />
      <path
        d="M16.2 13.5H19.8V17.7H24V21.3H19.8V25.5H16.2V21.3H12V17.7H16.2V13.5Z"
        fill="white"
      />
    </svg>
  );
}

export function SidebarHardwareDiskIcon({
  className,
}: {
  className?: string;
}): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      className={cn('size-[18px] shrink-0', className)}
      aria-hidden
    >
      <rect
        x="2.5"
        y="2.5"
        width="13"
        height="14.5"
        rx="2.2"
        fill="#DCE8FF"
        fillOpacity="0.85"
        stroke="currentColor"
        strokeWidth="1.35"
      />
      <circle cx="9" cy="8.5" r="3.2" stroke="currentColor" strokeWidth="1.25" fill="white" />
      <circle cx="9" cy="8.5" r="1" fill="#1677FF" />
      <line
        x1="5"
        y1="14.2"
        x2="8"
        y2="14.2"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <path
        d="M14 10.2L17.8 11.6V14.5C17.8 16.5 16.2 18 14 18.6C11.8 18 10.2 16.5 10.2 14.5V11.6L14 10.2Z"
        fill="#10B981"
      />
      <path
        d="M12.3 14.3L13.5 15.5L15.8 13.1"
        stroke="white"
        strokeWidth="1.35"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function SidebarSdCardIcon({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      className={cn('size-[18px] shrink-0', className)}
      aria-hidden
    >
      <path
        d="M5 2.5H12.2L15.5 5.8V16.2C15.5 17.1 14.8 17.8 13.9 17.8H5C4.1 17.8 3.4 17.1 3.4 16.2V4.1C3.4 3.2 4.1 2.5 5 2.5Z"
        fill="#E2ECFF"
        fillOpacity="0.85"
        stroke="currentColor"
        strokeWidth="1.35"
        strokeLinejoin="round"
      />
      <line
        x1="6.5"
        y1="5.2"
        x2="6.5"
        y2="8"
        stroke="#1677FF"
        strokeWidth="1.35"
        strokeLinecap="round"
      />
      <line
        x1="9"
        y1="5.2"
        x2="9"
        y2="8"
        stroke="#1677FF"
        strokeWidth="1.35"
        strokeLinecap="round"
      />
      <line
        x1="11.5"
        y1="5.2"
        x2="11.5"
        y2="8"
        stroke="#1677FF"
        strokeWidth="1.35"
        strokeLinecap="round"
      />
      <rect x="5.8" y="11" width="7.4" height="4.2" rx="1" fill="#93C5FD" opacity="0.7" />
    </svg>
  );
}

export function SidebarMoreToolsIcon({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      className={cn('size-[18px] shrink-0', className)}
      aria-hidden
    >
      <rect
        x="2.8"
        y="2.5"
        width="13.5"
        height="15"
        rx="2.2"
        fill="#E2ECFF"
        fillOpacity="0.85"
        stroke="currentColor"
        strokeWidth="1.35"
      />
      <path
        d="M5.2 10.2H7.6L8.9 7.2L10.8 13L12.1 10.2H14.2"
        stroke="#1677FF"
        strokeWidth="1.45"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function SidebarFileRepairIcon({
  className,
}: {
  className?: string;
}): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      className={cn('size-[18px] shrink-0', className)}
      aria-hidden
    >
      <rect
        x="3"
        y="2.5"
        width="11.5"
        height="14.5"
        rx="2"
        fill="#E2ECFF"
        fillOpacity="0.85"
        stroke="currentColor"
        strokeWidth="1.35"
      />
      <line
        x1="5.8"
        y1="6.5"
        x2="11.5"
        y2="6.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <line
        x1="5.8"
        y1="9.5"
        x2="9.8"
        y2="9.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <circle cx="14.2" cy="14.2" r="4.2" fill="#60A5FA" stroke="white" strokeWidth="1.2" />
      <path d="M12.8 15.6L15.5 12.9" stroke="white" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  );
}

export function SidebarLostPartitionIcon({
  className,
}: {
  className?: string;
}): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      className={cn('size-[18px] shrink-0', className)}
      aria-hidden
    >
      <rect
        x="2.5"
        y="2.5"
        width="12.5"
        height="14.5"
        rx="2"
        fill="#E2ECFF"
        fillOpacity="0.85"
        stroke="currentColor"
        strokeWidth="1.35"
      />
      <line
        x1="5.2"
        y1="6.2"
        x2="11.8"
        y2="6.2"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <circle cx="13.8" cy="13.5" r="3.6" fill="#93C5FD" stroke="#1677FF" strokeWidth="1.35" />
      <line
        x1="16.5"
        y1="16.2"
        x2="18.3"
        y2="18"
        stroke="#1677FF"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function SidebarChronoSnapIcon({
  className,
}: {
  className?: string;
}): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      className={cn('size-[18px] shrink-0', className)}
      aria-hidden
    >
      <path
        d="M2.5 5C2.5 3.9 3.4 3 4.5 3H8.2L9.8 4.8H15.5C16.6 4.8 17.5 5.7 17.5 6.8V14.5C17.5 15.6 16.6 16.5 15.5 16.5H4.5C3.4 16.5 2.5 15.6 2.5 14.5V5Z"
        fill="#E2ECFF"
        fillOpacity="0.85"
        stroke="currentColor"
        strokeWidth="1.35"
      />
      <circle cx="14" cy="13.8" r="4.2" fill="white" stroke="#1677FF" strokeWidth="1.35" />
      <path
        d="M14 11.8V14L15.4 14.9"
        stroke="#1677FF"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function Drive3dIcon({
  isSystemDrive,
  bitlocker,
  bitlockerLocked,
  className,
}: {
  isSystemDrive?: boolean;
  bitlocker?: boolean | 'unlocked' | 'locked';
  bitlockerLocked?: boolean;
  className?: string;
}): React.JSX.Element {
  const isLocked = bitlocker === 'locked' || Boolean(bitlockerLocked);
  return (
    <div
      className={cn('relative flex size-12 shrink-0 items-center justify-center', className)}
      aria-hidden
    >
      <svg viewBox="0 0 48 48" fill="none" className="size-12">
        <defs>
          <linearGradient
            id="hdd-top"
            x1="6"
            y1="8"
            x2="42"
            y2="38"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="#F1F5F9" />
            <stop offset="100%" stopColor="#CBD5E1" />
          </linearGradient>
          <linearGradient
            id="hdd-front"
            x1="6"
            y1="28"
            x2="42"
            y2="38"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="#64748B" />
            <stop offset="100%" stopColor="#475569" />
          </linearGradient>
        </defs>
        <path
          d="M10 12C10.8 10.2 12.5 9 14.5 9H33.5C35.5 9 37.2 10.2 38 12L43 27H5L10 12Z"
          fill="url(#hdd-top)"
          stroke="#94A3B8"
          strokeWidth="1.2"
        />
        <ellipse cx="24" cy="18.5" rx="9" ry="4.5" fill="white" fillOpacity="0.7" />
        <rect x="5" y="26" width="38" height="11" rx="3.5" fill="url(#hdd-front)" />
        <rect x="9" y="30" width="16" height="3" rx="1.5" fill="#94A3B8" fillOpacity="0.6" />
        <circle cx="36.5" cy="31.5" r="2.2" fill="#38BDF8" />
      </svg>
      {isSystemDrive ? (
        <span className="absolute left-0.5 top-0.5 grid size-5 grid-cols-2 gap-[1.5px] rounded-[4px] bg-white p-1 shadow-xs ring-1 ring-[#BFDBFE]">
          <span className="bg-[#0078D4]" />
          <span className="bg-[#0078D4]" />
          <span className="bg-[#0078D4]" />
          <span className="bg-[#0078D4]" />
        </span>
      ) : null}
      {bitlocker ? (
        <svg
          viewBox="0 0 16 16"
          fill="none"
          className="absolute bottom-0.5 right-0 size-[17px] bg-transparent drop-shadow-[0_1px_1.5px_rgba(15,23,42,0.25)]"
          aria-hidden
        >
          {isLocked ? (
            /* Closed shackle (BitLocker locked) */
            <path
              d="M4.8 7V4.4C4.8 2.6 6.2 1.2 8 1.2C9.8 1.2 11.2 2.6 11.2 4.4V7"
              stroke="#64748B"
              strokeWidth="1.65"
              strokeLinecap="round"
            />
          ) : (
            /* Open shackle (BitLocker unlocked) */
            <path
              d="M4.8 7V4.2C4.8 2.4 6.2 1.1 8 1.1C9.5 1.1 10.7 2 11.1 3.3"
              stroke="#64748B"
              strokeWidth="1.65"
              strokeLinecap="round"
            />
          )}
          <rect
            x="3"
            y="6.6"
            width="10"
            height="7.6"
            rx="1.6"
            fill={isLocked ? '#F59E0B' : '#0078D4'}
            stroke="white"
            strokeWidth="0.9"
          />
          <circle cx="8" cy="10" r="1.15" fill="white" />
          <path d="M8 10.8V12.4" stroke="white" strokeWidth="1.1" strokeLinecap="round" />
        </svg>
      ) : null}
    </div>
  );
}

export function RecycleBin3dIcon({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 44 44"
      fill="none"
      className={cn('size-11 shrink-0', className)}
      aria-hidden
    >
      <path
        d="M11 11H33L30.5 36.5C30.3 38.2 28.9 39.5 27.2 39.5H16.8C15.1 39.5 13.7 38.2 13.5 36.5L11 11Z"
        fill="#E2E8F0"
        stroke="#94A3B8"
        strokeWidth="1.2"
      />
      <ellipse cx="22" cy="11" rx="11" ry="3.2" fill="#94A3B8" />
      <path
        d="M19 22L22 18L25 22M25 26L22 30L19 26"
        stroke="#1677FF"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function Desktop3dIcon({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 44 44"
      fill="none"
      className={cn('size-11 shrink-0', className)}
      aria-hidden
    >
      <rect
        x="6"
        y="8"
        width="32"
        height="22"
        rx="3"
        fill="#CBD5E1"
        stroke="#94A3B8"
        strokeWidth="1.2"
      />
      <rect x="8.5" y="10.5" width="27" height="17" rx="1.5" fill="#E0F2FE" />
      <rect x="11" y="13" width="4" height="4" rx="0.8" fill="#1677FF" />
      <rect x="11" y="19" width="4" height="4" rx="0.8" fill="#60A5FA" />
      <path d="M18 30H26L28 35H16L18 30Z" fill="#94A3B8" />
      <rect x="13" y="35" width="18" height="2.2" rx="1.1" fill="#64748B" />
    </svg>
  );
}

export function SelectFolder3dIcon({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      viewBox="0 0 44 44"
      fill="none"
      className={cn('size-11 shrink-0', className)}
      aria-hidden
    >
      <path
        d="M6 12C6 10.3 7.3 9 9 9H17.5L20.5 12.5H35C36.7 12.5 38 13.8 38 15.5V32C38 33.7 36.7 35 35 35H9C7.3 35 6 33.7 6 32V12Z"
        fill="#CBD5E1"
      />
      <rect x="9" y="14" width="26" height="16" rx="2" fill="#1677FF" />
      <path
        d="M6 18.5C6 17.1 7.1 16 8.5 16H35.5C36.9 16 38 17.1 38 18.5L36.5 33C36.3 34.1 35.4 35 34.2 35H9.8C8.6 35 7.7 34.1 7.5 33L6 18.5Z"
        fill="#E2E8F0"
        stroke="#94A3B8"
        strokeWidth="1"
      />
    </svg>
  );
}

export function SaveDestinationIcon({
  withWarning,
  className,
}: {
  withWarning?: boolean;
  className?: string;
}): React.JSX.Element {
  return (
    <div
      className={cn(
        'relative inline-flex size-12 shrink-0 items-center justify-center',
        className,
      )}
      aria-hidden
    >
      <svg viewBox="0 0 48 48" fill="none" className="size-12">
        <rect x="6" y="6" width="36" height="36" rx="8" fill="#1677FF" />
        <rect x="13" y="6" width="18" height="12" rx="2" fill="#BFDBFE" />
        <rect x="25" y="8.5" width="3.5" height="7" rx="1" fill="#1677FF" />
        <rect x="12" y="24" width="24" height="14" rx="2.5" fill="white" fillOpacity="0.92" />
      </svg>
      {withWarning ? (
        <span className="absolute -bottom-1 -right-1 flex size-6 items-center justify-center rounded-full bg-[#F59E0B] text-white shadow-sm ring-2 ring-white">
          <span className="text-[12px] font-bold leading-none">!</span>
        </span>
      ) : null}
    </div>
  );
}
