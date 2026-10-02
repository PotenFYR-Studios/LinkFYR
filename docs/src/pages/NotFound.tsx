import { Link } from "react-router-dom";

export default function NotFound() {
  return (
    <main className="notfound">
      <h1>Page not found</h1>
      <p>
        That page does not exist. Start from the <Link to="/docs">documentation portal</Link>.
      </p>
    </main>
  );
}
