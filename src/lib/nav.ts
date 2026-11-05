/** The app's primary navigation, shared by the desktop bar and the mobile dock. */
export type NavItem = {
  href: string;
  label: string;
  /** Pixel-art icon used by the mobile dock. */
  icon: string;
};

export const NAV_ITEMS: NavItem[] = [
  { href: '/', label: 'Pet', icon: '/assets/button/home.png' },
  { href: '/stake', label: 'AI Lab', icon: '/assets/button/house.png' },
  { href: '/battle', label: 'Battle', icon: '/assets/button/attack.png' },
  { href: '/mining', label: 'Mining', icon: '/assets/button/mining.png' },
  { href: '/mint', label: 'Pets', icon: '/assets/button/petlist.png' },
  { href: '/market', label: 'Market', icon: '/assets/button/backpack.png' }
];

export const isActive = (href: string, pathname: string) =>
  href === '/' ? pathname === '/' : pathname.startsWith(href);
