import { useParams } from "react-router-dom";

export default function DocPage() {
  const { id } = useParams();
  return <main style={{ padding: 24 }}><h1>Doc {id}</h1></main>;
}
