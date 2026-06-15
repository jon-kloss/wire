import { NextResponse } from "next/server";

export async function GET(
  _request: Request,
  { params }: { params: { id: string } }
) {
  return NextResponse.json({ id: params.id });
}

export async function DELETE() {
  return new NextResponse(null, { status: 204 });
}
