import { fireEvent, render, screen } from "@testing-library/react";
import FilterableCombobox from "./FilterableCombobox";

const options = [
  { value: "alice.near", label: "Alice" },
  { value: "bob.near", label: "Bob" },
  { value: "charlie.near", label: "Charlie" },
];

function setup(props: Partial<React.ComponentProps<typeof FilterableCombobox>> = {}) {
  const onChange = vi.fn();
  const result = render(
    <FilterableCombobox
      label="Account"
      value="alice.near"
      options={options}
      onChange={onChange}
      {...props}
    />,
  );
  return { onChange, ...result };
}

describe("FilterableCombobox", () => {
  it("renders closed state with selected label", () => {
    setup();
    expect(screen.getByRole("combobox")).toHaveTextContent("Alice");
  });

  it("opens on click and shows all options", () => {
    setup();
    fireEvent.click(screen.getByRole("combobox"));
    expect(screen.getAllByRole("option")).toHaveLength(3);
  });

  it("typing filters to matching options only", () => {
    setup();
    fireEvent.click(screen.getByRole("combobox"));
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "bo" } });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByRole("option")).toHaveTextContent("Bob");
  });

  it("filters on value (not just label)", () => {
    setup();
    fireEvent.click(screen.getByRole("combobox"));
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "charlie.near" } });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByRole("option")).toHaveTextContent("Charlie");
  });

  it("click option calls onChange with correct value", () => {
    const { onChange } = setup();
    fireEvent.click(screen.getByRole("combobox"));
    fireEvent.mouseDown(screen.getByText("Bob"));
    expect(onChange).toHaveBeenCalledWith("bob.near");
  });

  it("ArrowDown/Up keyboard navigation", () => {
    setup();
    fireEvent.click(screen.getByRole("combobox"));
    const input = screen.getByRole("combobox");
    // Alice is index 0 (pre-highlighted as selected)
    fireEvent.keyDown(input, { key: "ArrowDown" });
    // Now Bob (index 1) should be highlighted
    const bobOption = screen.getAllByRole("option")[1];
    expect(bobOption.className).toContain("bg-blue-600");
    fireEvent.keyDown(input, { key: "ArrowUp" });
    const aliceOption = screen.getAllByRole("option")[0];
    expect(aliceOption.className).toContain("bg-blue-600");
  });

  it("Enter selects highlighted option", () => {
    const { onChange } = setup();
    fireEvent.click(screen.getByRole("combobox"));
    const input = screen.getByRole("combobox");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("bob.near");
  });

  it("Escape closes without selecting", () => {
    const { onChange } = setup();
    fireEvent.click(screen.getByRole("combobox"));
    const input = screen.getByRole("combobox");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("shows placeholder when no options", () => {
    setup({ options: [], value: "" });
    expect(screen.getByText("Not available")).toBeInTheDocument();
  });

  it("disabled state blocks opening", () => {
    setup({ disabled: true });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    // disabled renders a static div with placeholder
    const el = screen.getByText("Not available");
    expect(el.className).toContain("opacity-50");
  });
});
