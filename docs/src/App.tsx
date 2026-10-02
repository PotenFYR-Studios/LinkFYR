import { Link, NavLink, Route, Routes } from "react-router-dom";
import SeoManager from "./SeoManager";
import Home from "./pages/Home";
import Docs from "./pages/Docs";
import NotFound from "./pages/NotFound";

const GH = "https://github.com/PotenFYR-Studios/LinkFYR";

function Header() {
  return (
    <header className="site-header">
      <div className="site-header-inner">
        <Link className="brand" to="/">
          <span className="brand-mark" aria-hidden />
          <span className="brand-name">LinkFYR</span>
          <span className="brand-tag">docs</span>
        </Link>
        <nav className="site-nav" aria-label="Primary">
          <NavLink to="/" end className={({ isActive }) => (isActive ? "nav-active" : undefined)}>
            Home
          </NavLink>
          <NavLink to="/docs" className={({ isActive }) => (isActive ? "nav-active" : undefined)}>
            Documentation
          </NavLink>
          <a href={`${GH}/releases`}>Releases</a>
          <a href={GH} className="nav-strong">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}

function Footer() {
  return (
    <footer className="site-footer">
      <div className="site-footer-inner">
        <span>
          PotenFYR Studios · Apache-2.0 with the Commons Clause · free to use, not for resale
        </span>
        <span className="footer-links">
          <a href={`${GH}/blob/main/LICENSE`}>License</a>
          <a href={`${GH}/blob/main/SECURITY.md`}>Security</a>
          <a href={`${GH}/blob/main/CONTRIBUTING.md`}>Contributing</a>
        </span>
      </div>
    </footer>
  );
}

/** Router-agnostic shell: BrowserRouter lives in main.tsx, MemoryRouter in prerender. */
export default function App() {
  return (
    <div className="site">
      <SeoManager />
      <Header />
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/docs" element={<Docs />} />
        <Route path="/docs/:slug" element={<Docs />} />
        <Route path="*" element={<NotFound />} />
      </Routes>
      <Footer />
    </div>
  );
}
